use std::env;
use std::os::unix::process::CommandExt;
use std::process::Command;

#[allow(dead_code)]
mod util;

/// Subcommands implemented natively in Rust. Grows each migration phase.
const NATIVE: &[&str] = &[];

fn is_native(subcommand: &str) -> bool {
    NATIVE.contains(&subcommand)
}

/// First non-flag argument = the subcommand name, mirroring how argparse
/// resolves it on the Python side.
fn first_subcommand(args: &[String]) -> Option<&str> {
    args.iter().map(String::as_str).find(|a| !a.starts_with('-'))
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    match first_subcommand(&args) {
        Some(sub) if is_native(sub) => unreachable!("no native subcommands yet"),
        _ => delegate(&args),
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
    fn first_subcommand_skips_flags() {
        let args: Vec<String> = vec!["-v".into()];
        assert_eq!(first_subcommand(&args), None);

        let args: Vec<String> = vec!["shell".into(), "-d".into()];
        assert_eq!(first_subcommand(&args), Some("shell"));

        let args: Vec<String> = vec!["--version".into(), "toggle".into()];
        assert_eq!(first_subcommand(&args), Some("toggle"));

        let args: Vec<String> = vec![];
        assert_eq!(first_subcommand(&args), None);
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

    #[test]
    fn no_native_subcommands_in_phase_1() {
        for sub in ["shell", "toggle", "scheme", "screenshot", "record",
                    "clipboard", "emoji", "wallpaper", "resizer", "search",
                    "install", "update"] {
            assert!(!is_native(sub), "{sub} must delegate in phase 1");
        }
    }
}
