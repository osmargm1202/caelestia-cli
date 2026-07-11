use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::cli::ClipboardArgs;

// default.nix substitutes this literal with "caelestia-shell" (see
// shell.rs/screenshot.rs/search.rs).
const SHELL_CMD: &[&str] = &["qs", "-c", "caelestia"];

pub fn run(_args: ClipboardArgs) -> Result<()> {
    let status = Command::new(SHELL_CMD[0])
        .args(&SHELL_CMD[1..])
        .args(["ipc", "call", "launcher", "openClipboard"])
        .status()
        .context("failed to invoke clipboard IPC")?;
    if !status.success() {
        bail!("clipboard IPC failed: {status}");
    }
    Ok(())
}