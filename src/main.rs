use std::env;
use std::os::unix::process::CommandExt;
use std::process::Command;

use clap::Parser;

mod cli;
mod ipc;
mod subcommands;
mod util;

/// Subcommands implemented natively in Rust. Grows each migration phase.
const NATIVE: &[&str] = &["shell", "toggle", "screenshot", "record", "search", "clipboard", "emoji"];

fn is_native(subcommand: &str) -> bool {
    NATIVE.contains(&subcommand)
}

/// argv[0] is the subcommand IFF it is not a flag. Top-level flags
/// (-v/-h) always delegate — python still owns those until phase 6.
fn native_subcommand(args: &[String]) -> Option<&str> {
    match args.first() {
        Some(first) if !first.starts_with('-') && is_native(first) => Some(first),
        _ => None,
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if native_subcommand(&args).is_none() {
        delegate(&args);
    }

    let cli = cli::Cli::parse();
    let result = match cli.command {
        cli::Native::Shell(a) => subcommands::shell::run(a),
        cli::Native::Toggle(a) => subcommands::toggle::run(a),
        cli::Native::Screenshot(a) => subcommands::screenshot::run(a),
        cli::Native::Record(a) => subcommands::record::run(a),
        cli::Native::Search => subcommands::search::run(),
        cli::Native::Clipboard(a) => subcommands::clipboard::run(a),
        cli::Native::Emoji(a) => subcommands::emoji::run(a),
    };

    if let Err(e) = result {
        util::io::error(&format!("{e:#}"));
        std::process::exit(1);
    }
}

/// python-ref must win module resolution, but the caller's own
/// PYTHONPATH entries stay visible to the delegated process.
fn compute_pythonpath(ref_path: String, existing: Option<String>) -> String {
    match existing {
        Some(e) if !e.is_empty() => format!("{ref_path}:{e}"),
        _ => ref_path,
    }
}

/// Replace this process with the Python reference implementation.
/// exec() (not spawn) so stdin/tty, signals and exit codes pass through
/// untouched — interactive prompts in install/update keep working.
fn delegate(args: &[String]) -> ! {
    let python = env::var("CAELESTIA_PYTHON").unwrap_or_else(|_| "python3".into());
    let ref_path = env::var("CAELESTIA_PYTHONPATH")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/python-ref/src").into());
    let pythonpath = compute_pythonpath(ref_path, env::var("PYTHONPATH").ok());

    let err = Command::new(python)
        .arg("-m")
        .arg("caelestia")
        .args(args)
        .env("PYTHONPATH", pythonpath)
        .exec();

    eprintln!("caelestia: failed to launch python backend: {err}");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_set_matches_phase_2() {
        for sub in ["shell", "toggle", "screenshot", "record", "search", "clipboard", "emoji"] {
            assert!(is_native(sub), "{sub} must be native in phase 2");
        }
        for sub in ["scheme", "wallpaper", "resizer", "install", "update"] {
            assert!(!is_native(sub), "{sub} must still delegate in phase 2");
        }
    }

    #[test]
    fn top_level_flags_delegate() {
        let args: Vec<String> = vec!["-v".into()];
        assert_eq!(native_subcommand(&args), None);
        let args: Vec<String> = vec!["--version".into(), "toggle".into()];
        assert_eq!(native_subcommand(&args), None);
        let args: Vec<String> = vec!["toggle".into(), "comm".into()];
        assert_eq!(native_subcommand(&args), Some("toggle"));
        let args: Vec<String> = vec!["scheme".into(), "get".into()];
        assert_eq!(native_subcommand(&args), None);
    }

    #[test]
    fn pythonpath_prepends_ref_path_to_existing() {
        assert_eq!(
            compute_pythonpath("/ref".into(), Some("/user/lib".into())),
            "/ref:/user/lib"
        );
        assert_eq!(compute_pythonpath("/ref".into(), Some(String::new())), "/ref");
        assert_eq!(compute_pythonpath("/ref".into(), None), "/ref");
    }
}
