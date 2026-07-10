use std::process::{Command, Stdio};

use anyhow::{Context, Result};

/// notify-send wrapper; returns trimmed stdout (the action id / notif id).
pub fn notify(args: &[&str]) -> Result<String> {
    let out = Command::new("notify-send")
        .arg("-a")
        .arg("caelestia-cli")
        .args(args)
        .output()
        .context("failed to run notify-send")?;
    anyhow::ensure!(out.status.success(), "notify-send failed");
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[allow(dead_code)] // consumed by record.rs (Task 8)
pub fn close_notification(id: &str) -> Result<()> {
    Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest=org.freedesktop.Notifications",
            "--object-path=/org/freedesktop/Notifications",
            "--method=org.freedesktop.Notifications.CloseNotification",
            id,
        ])
        .stdout(Stdio::null())
        .status()
        .context("failed to run gdbus")?;
    Ok(())
}
