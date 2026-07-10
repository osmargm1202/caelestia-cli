use anyhow::Result;

use crate::cli::ClipboardArgs;

/// Removed in the NixOS fork: the shell launcher's clipboard UI
/// (C++ ClipboardCore) replaces the old cliphist+fuzzel picker.
pub fn run(_args: ClipboardArgs) -> Result<()> {
    anyhow::bail!("removed in this fork — use the shell launcher (clipboard tab) instead")
}
