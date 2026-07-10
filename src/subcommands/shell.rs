use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

use crate::cli::ShellArgs;
use crate::util::paths::c_cache_dir;

// default.nix substitutes this literal with "caelestia-shell" (same as
// the python patchPhase) — keep it byte-identical.
const SHELL_CMD: &[&str] = &["qs", "-c", "caelestia"];

fn shell_output(args: &[&str]) -> Result<String> {
    let out = Command::new(SHELL_CMD[0])
        .args(&SHELL_CMD[1..])
        .args(args)
        // python check_output only pipes stdout — stderr goes to the tty
        .stderr(Stdio::inherit())
        .output()
        .context("failed to run shell command")?;
    anyhow::ensure!(out.status.success(), "shell command failed");
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn filter_log(line: &str) -> bool {
    !line.contains(&format!(
        "Cannot open: file://{}/imagecache/",
        c_cache_dir().display()
    ))
}

pub fn run(args: ShellArgs) -> Result<()> {
    if args.show {
        print!("{}", shell_output(&["ipc", "show"])?);
    } else if args.log {
        let log = match &args.log_rules {
            Some(rules) => shell_output(&["log", "-r", rules])?,
            None => shell_output(&["log"])?,
        };
        for line in log.lines().filter(|l| filter_log(l)) {
            println!("{line}");
        }
    } else if args.kill {
        shell_output(&["kill"])?;
    } else if !args.message.is_empty() {
        let msg: Vec<&str> = std::iter::once("ipc")
            .chain(std::iter::once("call"))
            .chain(args.message.iter().map(String::as_str))
            .collect();
        print!("{}", shell_output(&msg)?);
    } else {
        let mut cmd = Command::new(SHELL_CMD[0]);
        cmd.args(&SHELL_CMD[1..]).arg("-n");
        if let Some(rules) = &args.log_rules {
            cmd.args(["--log-rules", rules]);
        }
        if args.daemon {
            cmd.arg("-d");
            cmd.status().context("failed to start shell daemon")?;
        } else {
            let mut child = cmd
                .stdout(Stdio::piped())
                .spawn()
                .context("failed to start shell")?;
            if let Some(stdout) = child.stdout.take() {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if filter_log(&line) {
                        println!("{line}");
                    }
                }
            }
        }
    }
    Ok(())
}
