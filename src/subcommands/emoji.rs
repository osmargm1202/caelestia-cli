use anyhow::Result;

use crate::cli::EmojiArgs;

/// Removed in the NixOS fork: the shell launcher's emoji picker
/// replaces the old fuzzel-based picker.
pub fn run(_args: EmojiArgs) -> Result<()> {
    anyhow::bail!("removed in this fork — use the shell launcher (emoji picker) instead")
}
