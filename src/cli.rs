use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "caelestia",
    disable_version_flag = true,
    infer_long_args = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Native,
}

#[derive(Subcommand)]
pub enum Native {
    /// start or message the shell
    Shell(ShellArgs),
    /// toggle a special workspace
    Toggle(ToggleArgs),
    /// take a screenshot
    Screenshot(ScreenshotArgs),
    /// start a screen recording
    Record(RecordArgs),
    /// search using a screen region
    Search,
    /// open clipboard history
    Clipboard(ClipboardArgs),
    /// emoji/glyph utilities
    Emoji(EmojiArgs),
    /// manage the colour scheme
    Scheme(SchemeActionArgs),
    /// inspect or change the wallpaper
    Wallpaper(WallpaperArgs),
    /// generate a colour scheme JSON from an image (used by golden parity tests)
    Golden(GoldenArgs),
}

#[derive(Args)]
pub struct ShellArgs {
    /// a message to send to the shell
    pub message: Vec<String>,
    /// start the shell detached
    #[arg(short, long)]
    pub daemon: bool,
    /// print all shell IPC commands
    #[arg(short, long)]
    pub show: bool,
    /// print the shell log
    #[arg(short, long)]
    pub log: bool,
    /// kill the shell
    #[arg(short, long)]
    pub kill: bool,
    /// log rules to apply
    #[arg(long, value_name = "RULES")]
    pub log_rules: Option<String>,
}

#[derive(Args)]
pub struct ToggleArgs {
    /// the workspace to toggle
    pub workspace: String,
}

#[derive(Args)]
pub struct ScreenshotArgs {
    /// take a screenshot of a region
    #[arg(short, long, num_args = 0..=1, default_missing_value = "slurp")]
    pub region: Option<String>,
    /// freeze the screen while selecting a region
    #[arg(short, long)]
    pub freeze: bool,
}

#[derive(Args)]
pub struct RecordArgs {
    /// record a region
    #[arg(short, long, num_args = 0..=1, default_missing_value = "slurp")]
    pub region: Option<String>,
    /// record audio
    #[arg(short, long)]
    pub sound: bool,
    /// pause/resume the recording
    #[arg(short, long)]
    pub pause: bool,
    /// copy recording path to clipboard
    #[arg(short, long)]
    pub clipboard: bool,
}

#[derive(Args)]
pub struct ClipboardArgs {
    /// delete from clipboard history
    #[arg(short, long)]
    pub delete: bool,
}

#[derive(Args)]
pub struct EmojiArgs {
    /// open the emoji/glyph picker
    #[arg(short, long)]
    pub picker: bool,
    /// fetch emoji/glyph data from remote
    #[arg(short, long)]
    pub fetch: bool,
}

#[derive(Args)]
pub struct GoldenArgs {
    #[arg(long)]
    pub image: String,
    #[arg(long, default_value = "tonalspot")]
    pub variant: String,
    #[arg(long, default_value = "default")]
    pub flavour: String,
    #[arg(long, default_value = "dark")]
    pub mode: String,
}

pub use crate::subcommands::scheme::SchemeActionArgs;
pub use crate::subcommands::wallpaper::WallpaperArgs;

#[cfg(test)]
mod tests {
    use super::*;

    /// argparse accepts unambiguous long-option prefixes; infer_long_args
    /// on Cli propagates to subcommands so clap matches that behavior.
    #[test]
    fn long_flag_prefixes_parse() {
        let cli = Cli::try_parse_from(["caelestia", "clipboard", "--del"]).unwrap();
        assert!(matches!(
            cli.command,
            Native::Clipboard(ClipboardArgs { delete: true })
        ));

        let cli = Cli::try_parse_from(["caelestia", "shell", "--dae"]).unwrap();
        assert!(matches!(
            cli.command,
            Native::Shell(ShellArgs { daemon: true, .. })
        ));

        let cli = Cli::try_parse_from(["caelestia", "record", "--pau"]).unwrap();
        assert!(matches!(
            cli.command,
            Native::Record(RecordArgs { pause: true, .. })
        ));
    }

    /// `--l` matches both --log and --log-rules — ambiguous prefixes must
    /// still error, exactly like argparse (exit 2 at the binary level).
    #[test]
    fn ambiguous_long_flag_prefix_errors() {
        assert!(Cli::try_parse_from(["caelestia", "shell", "--l"]).is_err());
    }

    #[test]
    fn wallpaper_file_parses() {
        let cli = Cli::try_parse_from(["caelestia", "wallpaper", "--file", "/tmp/a.png"]).unwrap();
        assert!(matches!(
            cli.command,
            Native::Wallpaper(WallpaperArgs { file: Some(path), .. }) if path == "/tmp/a.png"
        ));
    }

    #[test]
    fn wallpaper_optional_paths_and_short_flags_parse() {
        let cli = Cli::try_parse_from(["caelestia", "wallpaper", "-p"]).unwrap();
        assert!(matches!(
            cli.command,
            Native::Wallpaper(WallpaperArgs { print: Some(path), .. }) if path.is_empty()
        ));
        let cli = Cli::try_parse_from(["caelestia", "wallpaper", "-r", "-n", "-N"]).unwrap();
        assert!(matches!(
            cli.command,
            Native::Wallpaper(WallpaperArgs {
                random: Some(path),
                no_filter: true,
                no_smart: true,
                threshold,
                ..
            }) if path.is_empty() && threshold == 0.8
        ));
    }

    #[test]
    fn wallpaper_random_flags_parse() {
        let cli = Cli::try_parse_from([
            "caelestia",
            "wallpaper",
            "--random",
            "/walls",
            "--no-filter",
            "--threshold",
            "0.8",
            "--no-smart",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Native::Wallpaper(WallpaperArgs {
                random: Some(path),
                no_filter: true,
                threshold,
                no_smart: true,
                ..
            }) if path == "/walls" && threshold == 0.8
        ));
    }
}
