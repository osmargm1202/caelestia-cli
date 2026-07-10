use std::fmt::Write as FmtWrite;
use std::io::Write as IoWrite;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::Local;
use serde_json::Value;

use crate::cli::RecordArgs;
use crate::ipc::hypr;
use crate::util::notify::{close_notification, notify};
use crate::util::paths::{get_config, recording_notif_path, recording_path, recordings_dir};

const RECORDER: &str = "gpu-screen-recorder";
type Rect = (i64, i64, i64, i64);

fn intersects(a: Rect, b: Rect) -> bool {
    a.0 < b.0 + b.2 && a.0 + a.2 > b.0 && a.1 < b.1 + b.3 && a.1 + a.3 > b.1
}

fn parse_region(region: &str) -> Result<Rect> {
    let (size, position) = region
        .trim()
        .split_once('+')
        .ok_or_else(|| anyhow::anyhow!("Invalid region: {region}"))?;
    let (x, y) = position
        .split_once('+')
        .ok_or_else(|| anyhow::anyhow!("Invalid region: {region}"))?;
    let (width, height) = size
        .split_once('x')
        .ok_or_else(|| anyhow::anyhow!("Invalid region: {region}"))?;

    let parsed = (
        x.parse::<i64>(),
        y.parse::<i64>(),
        width.parse::<i64>(),
        height.parse::<i64>(),
    );
    match parsed {
        (Ok(x), Ok(y), Ok(width), Ok(height)) => Ok((x, y, width, height)),
        _ => bail!("Invalid region: {region}"),
    }
}

fn monitor_i64(monitor: &Value, key: &str) -> Result<i64> {
    monitor[key]
        .as_i64()
        .with_context(|| format!("monitor missing integer {key:?}"))
}

fn monitor_refresh_rate(monitor: &Value) -> Result<i64> {
    monitor["refreshRate"]
        .as_f64()
        .map(|rate| rate.round() as i64)
        .context("monitor missing refresh rate")
}

fn region_capture_args(region: &str, monitors: &[Value]) -> Result<Vec<String>> {
    let target = parse_region(region)?;
    let mut max_refresh_rate = 0;

    for monitor in monitors {
        let bounds = (
            monitor_i64(monitor, "x")?,
            monitor_i64(monitor, "y")?,
            monitor_i64(monitor, "width")?,
            monitor_i64(monitor, "height")?,
        );
        if intersects(bounds, target) {
            max_refresh_rate = max_refresh_rate.max(monitor_refresh_rate(monitor)?);
        }
    }

    Ok(vec![
        "region".into(),
        "-region".into(),
        region.into(),
        "-f".into(),
        max_refresh_rate.to_string(),
    ])
}

fn fullscreen_capture_args(monitors: &[Value]) -> Result<Vec<String>> {
    let focused = monitors
        .iter()
        .find(|monitor| monitor["focused"].as_bool().unwrap_or(false))
        .context("no focused monitor found")?;
    let name = focused["name"]
        .as_str()
        .context("focused monitor missing name")?;

    Ok(vec![
        name.into(),
        "-f".into(),
        monitor_refresh_rate(focused)?.to_string(),
    ])
}

fn config_extra_args(config: &Value) -> Result<Vec<String>> {
    let Some(extra_args) = config
        .get("record")
        .and_then(|record| record.get("extraArgs"))
    else {
        return Ok(Vec::new());
    };
    let args = extra_args.as_array().ok_or_else(|| {
        anyhow::anyhow!(
            "Config option 'record.extraArgs' should be an array: expected an array of strings"
        )
    })?;

    args.iter()
        .map(|arg| {
            arg.as_str().map(str::to_owned).ok_or_else(|| {
                anyhow::anyhow!(
                    "Config option 'record.extraArgs' should be an array: expected an array of strings"
                )
            })
        })
        .collect()
}

fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            let _ = write!(uri, "%{byte:02X}");
        }
    }
    uri.push('\n');
    uri
}

fn proc_running() -> Result<bool> {
    Ok(Command::new("pidof")
        .arg(RECORDER)
        .stdout(Stdio::null())
        .status()
        .context("failed to check recorder process")?
        .success())
}

fn resolve_region(region: &str) -> Result<String> {
    if region != "slurp" {
        return Ok(region.trim().to_owned());
    }

    let output = Command::new("slurp")
        .args(["-f", "%wx%h+%x+%y"])
        .output()
        .context("failed to select recording region")?;
    anyhow::ensure!(output.status.success(), "slurp failed");
    String::from_utf8(output.stdout).context("slurp returned a non-UTF-8 region")
}

fn start(args: &RecordArgs) -> Result<()> {
    let monitors = hypr::message_json("monitors")?;
    let monitors = monitors.as_array().context("invalid monitors response")?;

    let mut recorder_args = vec!["-w".to_owned()];
    match args.region.as_deref() {
        Some(region) => {
            let region = resolve_region(region)?;
            recorder_args.extend(region_capture_args(&region, monitors)?);
        }
        None => recorder_args.extend(fullscreen_capture_args(monitors)?),
    }
    if args.sound {
        recorder_args.extend(["-a".into(), "default_output".into()]);
    }
    recorder_args.extend(config_extra_args(&get_config())?);

    let output_path = recording_path();
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let command_text = std::iter::once(RECORDER.to_owned())
        .chain(recorder_args.iter().cloned())
        .chain(["-o".into(), output_path.to_string_lossy().into_owned()])
        .collect::<Vec<_>>()
        .join(" ");
    let mut process = Command::new(RECORDER)
        .args(&recorder_args)
        .arg("-o")
        .arg(&output_path)
        .process_group(0)
        .spawn()
        .context("failed to start recorder")?;

    let notification_id = notify(&["-p", "Recording started", "Recording..."])?;
    std::fs::write(recording_notif_path(), &notification_id)?;

    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match process
            .try_wait()
            .context("failed to poll recorder process")?
        {
            Some(status) => {
                if !status.success() {
                    close_notification(&notification_id)?;
                    let exit_code = status
                        .code()
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| status.to_string());
                    let body = format!(
                        "An error occurred attempting to start recorder. Command `{command_text}` failed with exit code {exit_code}"
                    );
                    notify(&["Recording failed", &body])?;
                }
                break;
            }
            None if Instant::now() >= deadline => break,
            None => std::thread::sleep(Duration::from_millis(25)),
        }
    }

    Ok(())
}

fn move_recording(source: &Path, destination: &Path) -> Result<()> {
    match std::fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(rename_error) => {
            std::fs::copy(source, destination).with_context(|| {
                format!(
                    "failed to move recording from {} to {} after rename failed: {rename_error}",
                    source.display(),
                    destination.display()
                )
            })?;
            std::fs::remove_file(source)?;
            Ok(())
        }
    }
}

fn spawn_detached(program: &str, arg: &Path) -> Result<()> {
    Command::new(program)
        .arg(arg)
        .process_group(0)
        .spawn()
        .with_context(|| format!("failed to start {program}"))?;
    Ok(())
}

fn copy_recording_uri(path: &Path) -> Result<()> {
    let absolute = path
        .canonicalize()
        .with_context(|| format!("failed to resolve recording path {}", path.display()))?;
    let mut child = Command::new("wl-copy")
        .args(["--type", "text/uri-list"])
        .stdin(Stdio::piped())
        .spawn()
        .context("failed to start wl-copy")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(file_uri(&absolute).as_bytes())
            .context("failed to copy recording URI")?;
    }
    let _ = child.wait();
    Ok(())
}

fn stop(args: &RecordArgs) -> Result<()> {
    Command::new("pkill")
        .args(["-f", RECORDER])
        .stdout(Stdio::null())
        .status()
        .context("failed to stop recorder")?;

    while proc_running()? {
        std::thread::sleep(Duration::from_millis(100));
    }

    let directory = recordings_dir();
    std::fs::create_dir_all(&directory)?;
    let destination = directory.join(format!(
        "recording_{}.mp4",
        Local::now().format("%Y%m%d_%H-%M-%S")
    ));
    move_recording(&recording_path(), &destination)?;

    if let Ok(notification_id) = std::fs::read_to_string(recording_notif_path()) {
        close_notification(&notification_id)?;
    }

    if args.clipboard {
        copy_recording_uri(&destination)?;
    }

    let body = format!("Recording saved in {}", destination.display());
    let action = notify(&[
        "--action=watch=Watch",
        "--action=open=Open",
        "--action=delete=Delete",
        "Recording stopped",
        &body,
    ])?;

    match action.as_str() {
        "watch" => spawn_detached("xdg-open", &destination)?,
        "open" => {
            let item = format!("array:string:file://{}", destination.display());
            let status = Command::new("dbus-send")
                .args([
                    "--session",
                    "--dest=org.freedesktop.FileManager1",
                    "--type=method_call",
                    "/org/freedesktop/FileManager1",
                    "org.freedesktop.FileManager1.ShowItems",
                    &item,
                    "string:",
                ])
                .status()
                .context("failed to show recording in file manager")?;
            if !status.success() {
                spawn_detached(
                    "xdg-open",
                    destination
                        .parent()
                        .context("recording path has no parent")?,
                )?;
            }
        }
        "delete" => std::fs::remove_file(&destination)?,
        _ => {}
    }

    Ok(())
}

pub fn run(args: RecordArgs) -> Result<()> {
    if args.pause {
        Command::new("pkill")
            .args(["-USR2", "-f", RECORDER])
            .stdout(Stdio::null())
            .status()
            .context("failed to pause or resume recorder")?;
        Ok(())
    } else if proc_running()? {
        stop(&args)
    } else {
        start(&args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_intersection_matches_python() {
        assert!(intersects((0, 0, 100, 100), (50, 50, 100, 100)));
        assert!(!intersects((0, 0, 10, 10), (20, 20, 5, 5)));
        assert!(!intersects((0, 0, 10, 10), (10, 0, 10, 10)));
    }

    #[test]
    fn region_parses() {
        assert_eq!(parse_region("1920x1080+0+0").unwrap(), (0, 0, 1920, 1080));
        assert!(parse_region("garbage").is_err());
    }

    #[test]
    fn region_uses_max_refresh_rate_across_intersecting_monitors() {
        let monitors = serde_json::json!([
            {"x": 0, "y": 0, "width": 100, "height": 100, "refreshRate": 60.0},
            {"x": 100, "y": 0, "width": 100, "height": 100, "refreshRate": 144.0}
        ]);
        assert_eq!(
            region_capture_args("150x100+50+0", monitors.as_array().unwrap()).unwrap(),
            ["region", "-region", "150x100+50+0", "-f", "144"]
        );
    }

    #[test]
    fn record_extra_args_must_be_an_array_of_strings() {
        assert_eq!(
            config_extra_args(&serde_json::json!({"record": {"extraArgs": ["-k", "hevc"]}}))
                .unwrap(),
            ["-k", "hevc"]
        );
        assert!(
            config_extra_args(&serde_json::json!({"record": {"extraArgs": "-k hevc"}}))
                .unwrap_err()
                .to_string()
                .starts_with("Config option 'record.extraArgs' should be an array")
        );
    }

    #[test]
    fn clipboard_uri_matches_python_path_as_uri() {
        assert_eq!(
            file_uri(Path::new("/tmp/a recording.mp4")),
            "file:///tmp/a%20recording.mp4\n"
        );
    }
}
