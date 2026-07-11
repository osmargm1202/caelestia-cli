use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::cli::EmojiArgs;

// default.nix substitutes this literal with "caelestia-shell" (see
// shell.rs/screenshot.rs/search.rs).
const SHELL_CMD: &[&str] = &["qs", "-c", "caelestia"];

pub fn run(_args: EmojiArgs) -> Result<()> {
    let status = Command::new(SHELL_CMD[0])
        .args(&SHELL_CMD[1..])
        .args(["ipc", "call", "launcher", "openEmoji"])
        .status()
        .context("failed to invoke emoji IPC")?;
    if !status.success() {
        bail!("emoji IPC failed: {status}");
    }
    Ok(())
}
