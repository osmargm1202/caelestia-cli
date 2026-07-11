use std::process::Command;

// CAELESTIA_PYTHON=echo turns the delegation into an observable echo of
// exactly what would be exec'd, without needing a real Python env.

#[test]
fn delegates_full_argv_to_python_backend() {
    let out = Command::new(env!("CARGO_BIN_EXE_caelestia"))
        .args(["wallpaper", "list", "-n"])
        .env("CAELESTIA_PYTHON", "echo")
        .output()
        .expect("failed to run caelestia binary");

    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout.trim(), "-m caelestia wallpaper list -n");
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
    for (command, pointer) in [
        ("clipboard", "shell launcher (clipboard tab)"),
        ("emoji", "shell launcher (emoji picker)"),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_caelestia"))
            .arg(command)
            .output()
            .expect("failed to run caelestia binary");

        assert!(!out.status.success(), "{command} must exit nonzero");
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert!(
            stderr.contains("removed in this fork"),
            "stderr: {stderr:?}"
        );
        assert!(stderr.contains(pointer), "stderr: {stderr:?}");
    }
}
