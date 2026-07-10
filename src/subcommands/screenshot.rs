use std::io::Write;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use chrono::Local;

use crate::cli::ScreenshotArgs;
use crate::ipc::hypr;
use crate::util::notify::notify;
use crate::util::paths::{screenshots_cache_dir, screenshots_dir};

// default.nix substitutes this literal with "caelestia-shell" (same as
// the python patchPhase) — keep it byte-identical.
const SHELL_CMD: &[&str] = &["qs", "-c", "caelestia"];

pub fn run(args: ScreenshotArgs) -> Result<()> {
    match &args.region {
        Some(region) => region_screenshot(region, args.freeze),
        None => fullscreen(),
    }
}

fn region_screenshot(region: &str, freeze: bool) -> Result<()> {
    if region == "slurp" {
        // python: subprocess.run(...) — not checked, fire-and-wait.
        Command::new(SHELL_CMD[0])
            .args(&SHELL_CMD[1..])
            .args([
                "ipc",
                "call",
                "picker",
                if freeze { "openFreeze" } else { "open" },
            ])
            .status()
            .context("failed to open region picker")?;
        return Ok(());
    }

    let out = Command::new("grim")
        .args(["-l", "0", "-g", region.trim(), "-"])
        // python check_output only pipes stdout — stderr goes to the tty
        .stderr(Stdio::inherit())
        .output()
        .context("failed to capture region screenshot")?;
    anyhow::ensure!(out.status.success(), "grim failed");

    let mut swappy = Command::new("swappy")
        .args(["-f", "-"])
        .stdin(Stdio::piped())
        .process_group(0)
        .spawn()
        .context("failed to start swappy")?;

    if let Some(mut stdin) = swappy.stdin.take() {
        stdin
            .write_all(&out.stdout)
            .context("failed to write to swappy stdin")?;
    }
    Ok(())
}

fn fullscreen() -> Result<()> {
    let monitors = hypr::message_json("monitors")?;
    let focused = monitors
        .as_array()
        .and_then(|arr| arr.iter().find(|m| m["focused"].as_bool().unwrap_or(false)))
        .context("no focused monitor found")?;
    let name = focused["name"]
        .as_str()
        .context("focused monitor missing name")?;

    let out = Command::new("grim")
        .args(["-o", name, "-"])
        .stderr(Stdio::inherit())
        .output()
        .context("failed to capture screenshot")?;
    anyhow::ensure!(out.status.success(), "grim failed");
    let sc_data = out.stdout;

    let mut wl_copy = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .spawn()
        .context("failed to start wl-copy")?;
    if let Some(mut stdin) = wl_copy.stdin.take() {
        stdin
            .write_all(&sc_data)
            .context("failed to write to wl-copy stdin")?;
    }
    let _ = wl_copy.wait();

    let cache_dir = screenshots_cache_dir();
    std::fs::create_dir_all(&cache_dir)?;
    let dest = cache_dir.join(Local::now().format("%Y%m%d%H%M%S").to_string());
    std::fs::write(&dest, &sc_data)?;

    let action = notify(&[
        "-i",
        "image-x-generic-symbolic",
        "-h",
        &format!("STRING:image-path:{}", dest.display()),
        "--action=open=Open",
        "--action=save=Save",
        "Screenshot taken",
        &format!(
            "Screenshot stored in {} and copied to clipboard",
            dest.display()
        ),
    ])?;

    match action.as_str() {
        "open" => {
            Command::new("swappy")
                .args([
                    "-f",
                    dest.to_str()
                        .context("screenshot path is not valid UTF-8")?,
                ])
                .process_group(0)
                .spawn()
                .context("failed to start swappy")?;
        }
        "save" => {
            let mut new_dest = screenshots_dir().join(
                dest.file_name()
                    .context("screenshot path missing filename")?,
            );
            new_dest.set_extension("png");
            if let Some(parent) = new_dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&dest, &new_dest)?;
            notify(&[
                "Screenshot saved",
                &format!("Saved to {}", new_dest.display()),
            ])?;
        }
        _ => {}
    }

    Ok(())
}
