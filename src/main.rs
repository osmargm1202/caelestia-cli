use clap::Parser;

mod cli;
mod core;
mod ipc;
mod subcommands;
mod util;

fn main() {
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
        cli::Native::Resizer(a) => subcommands::resizer::run(a),
        cli::Native::Golden(a) => subcommands::golden::run(a),
    };

    if let Err(e) = result {
        util::io::error(&format!("{e:#}"));
        std::process::exit(1);
    }
}

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
