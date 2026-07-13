use std::env;
use std::os::unix::process::CommandExt;
use std::process::Command;

use clap::Parser;

mod cli;
mod core;
mod ipc;
mod subcommands;
mod util;

#[cfg(test)]
mod test_support {
    use std::ffi::{OsStr, OsString};
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    pub struct EnvGuard {
        _lock: MutexGuard<'static, ()>,
        saved: Vec<(&'static str, Option<OsString>)>,
    }

    impl EnvGuard {
        pub fn new() -> Self {
            Self {
                _lock: ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner()),
                saved: Vec::new(),
            }
        }

        pub fn set(&mut self, key: &'static str, value: impl AsRef<OsStr>) {
            if !self.saved.iter().any(|(saved_key, _)| *saved_key == key) {
                self.saved.push((key, std::env::var_os(key)));
            }
            std::env::set_var(key, value);
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in self.saved.drain(..).rev() {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

/// Subcommands implemented natively in Rust. Grows each migration phase.
const NATIVE: &[&str] = &[
    "shell",
    "toggle",
    "screenshot",
    "record",
    "search",
    "clipboard",
    "emoji",
    "scheme",
    "wallpaper",
    "golden",
];

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
        cli::Native::Scheme(a) => subcommands::scheme::run(a),
        cli::Native::Wallpaper(a) => subcommands::wallpaper::run(a),
        cli::Native::Golden(a) => subcommands::golden::run(a),
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
        for sub in [
            "shell",
            "toggle",
            "screenshot",
            "record",
            "search",
            "clipboard",
            "emoji",
            "scheme",
        ] {
            assert!(is_native(sub), "{sub} must be native in phase 2");
        }
        assert!(is_native("wallpaper"));
        for sub in ["resizer", "install", "update"] {
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
        assert_eq!(native_subcommand(&args), Some("scheme"));
    }

    #[test]
    fn pythonpath_prepends_ref_path_to_existing() {
        assert_eq!(
            compute_pythonpath("/ref".into(), Some("/user/lib".into())),
            "/ref:/user/lib"
        );
        assert_eq!(
            compute_pythonpath("/ref".into(), Some(String::new())),
            "/ref"
        );
        assert_eq!(compute_pythonpath("/ref".into(), None), "/ref");
    }
}
