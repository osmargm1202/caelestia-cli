use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn write_fake_qs(dir: &Path) {
    let bin = dir.join("qs");
    let script = "printf 'argv=%s\\n' \"$*\" >> \"$TRACE\"\n";
    fs::write(
        &bin,
        format!("#!/bin/sh\nset -e\nTRACE=\"$TRACE\"\n{script}"),
    )
    .unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
}

fn run_with_fake_qs(qs_dir: &Path, trace: &Path, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_caelestia"));
    cmd.args(args)
        .env("PATH", qs_dir.as_os_str())
        .env("TRACE", trace);
    cmd.output().expect("failed to run caelestia binary")
}

#[test]
fn clipboard_forwards_to_shell_launcher_ipc() {
    let tmp = tempdir();
    let qs = tmp.join("bin");
    fs::create_dir_all(&qs).unwrap();
    write_fake_qs(&qs);
    let trace = tmp.join("argv.txt");

    let out = run_with_fake_qs(&qs, &trace, &["clipboard"]);
    assert!(out.status.success(), "clipboard must exit 0: {:?}", out);
    let recorded = fs::read_to_string(&trace).unwrap();
    assert!(
        recorded.contains("argv=-c caelestia ipc call launcher openClipboard"),
        "argv={recorded:?}"
    );
}

#[test]
fn emoji_forwards_to_shell_launcher_ipc() {
    let tmp = tempdir();
    let qs = tmp.join("bin");
    fs::create_dir_all(&qs).unwrap();
    write_fake_qs(&qs);
    let trace = tmp.join("argv.txt");

    let out = run_with_fake_qs(&qs, &trace, &["emoji"]);
    assert!(out.status.success(), "emoji must exit 0: {:?}", out);
    let recorded = fs::read_to_string(&trace).unwrap();
    assert!(
        recorded.contains("argv=-c caelestia ipc call launcher openEmoji"),
        "argv={recorded:?}"
    );
}

fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("caelestia-ipc-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}
