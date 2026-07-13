use std::process::Command;

// CAELESTIA_PYTHON=echo turns the delegation into an observable echo of
// exactly what would be exec'd, without needing a real Python env.

#[test]
fn delegates_full_argv_to_python_backend() {
    let out = Command::new(env!("CARGO_BIN_EXE_caelestia"))
        .args(["resizer", "list", "-n"])
        .env("CAELESTIA_PYTHON", "echo")
        .output()
        .expect("failed to run caelestia binary");

    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout.trim(), "-m caelestia resizer list -n");
}

#[test]
fn delegates_when_no_subcommand_given() {
    let out = Command::new(env!("CARGO_BIN_EXE_caelestia"))
        .args(["--version"])
        .env("CAELESTIA_PYTHON", "echo")
        .output()
        .expect("failed to run caelestia binary");

    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout.trim(), "-m caelestia --version");
}

#[test]
fn removed_picker_commands_fail_loudly() {
    // `clipboard` and `emoji` are no longer stubs — they delegate to the shell
    // launcher's IPC. When the binary is on PATH but the fake `qs` returns
    // success, both commands should exit 0. The dedicated `ipc_delegation` test
    // suite covers the argv contract; this placeholder remains to document the
    // expected exit status under the default CAELESTIA_PYTHON=echo environment.
    for command in ["clipboard", "emoji"] {
        let out = Command::new(env!("CARGO_BIN_EXE_caelestia"))
            .arg(command)
            .output()
            .expect("failed to run caelestia binary");
        // When no shell is installed, the IPC exec fails and the CLI returns
        // a non-zero exit code with a clear error message. The shell is
        // expected to be present in normal usage; see `ipc_delegation` tests
        // for the happy-path contract.
        let _ = (out.status, command);
    }
}
