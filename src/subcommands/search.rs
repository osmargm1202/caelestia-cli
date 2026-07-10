use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

const SEARCH_PNG: &str = "/tmp/caelestia-search.png";
const SEARCH_DONE: &str = "/tmp/caelestia-search.done";

// default.nix substitutes this literal (see shell.rs).
const SHELL_CMD: &[&str] = &["qs", "-c", "caelestia"];

pub fn run() -> Result<()> {
    let _ = std::fs::remove_file(SEARCH_PNG);
    let _ = std::fs::remove_file(SEARCH_DONE);

    Command::new(SHELL_CMD[0])
        .args(&SHELL_CMD[1..])
        .args(["ipc", "call", "picker", "openSearch"])
        .process_group(0)
        .spawn()
        .context("failed to open search picker")?;

    let mut found = false;
    for _ in 0..100 {
        if Path::new(SEARCH_DONE).exists() {
            found = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    if !found {
        return Ok(());
    }
    let _ = std::fs::remove_file(SEARCH_DONE);

    let out = Command::new("curl")
        .args([
            "-sSf",
            "--connect-timeout",
            "5",
            "--max-time",
            "15",
            "-F",
            &format!("files[]=@{SEARCH_PNG}"),
            "https://uguu.se/upload",
        ])
        .output()
        .context("failed to upload search image")?;
    anyhow::ensure!(out.status.success(), "upload failed");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    if let Some(url) = v["files"][0]["url"].as_str().filter(|u| !u.is_empty()) {
        Command::new("xdg-open")
            .arg(format!("https://lens.google.com/uploadbyurl?url={url}"))
            .process_group(0)
            .spawn()?;
    }
    Ok(())
}
