use std::path::{Path, PathBuf};
use std::process::Command;

fn tempdir(suffix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "caelestia-scheme-preview-{suffix}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_caelestia(env_root: &Path, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_caelestia"));
    cmd.args(args)
        .env("XDG_DATA_HOME", env_root.join("data"))
        .env("XDG_CONFIG_HOME", env_root.join("cfg"))
        .env("XDG_STATE_HOME", env_root.join("state"));
    cmd.output().expect("failed to run caelestia binary")
}

#[test]
fn preview_emits_single_json_with_requested_variant() {
    let root = tempdir("json");
    // Surface the scheme fixtures so the test does not depend on the host.
    let ship = std::env::current_dir()
        .unwrap()
        .join("tests/fixtures/schemes");
    let scheme_dest = root.join("data/caelestia/schemes");
    std::fs::create_dir_all(scheme_dest.parent().unwrap()).unwrap();
    let _ = std::fs::remove_dir_all(&scheme_dest);
    let status = std::process::Command::new("cp")
        .args([
            "-a",
            ship.to_str().unwrap(),
            scheme_dest.parent().unwrap().to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success(), "failed to copy schemes data");

    let out = run_caelestia(&root, &["scheme", "preview", "--variant", "tonalspot"]);
    assert!(out.status.success(), "preview must succeed: {:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout is JSON");
    assert_eq!(value["variant"], "tonalspot");
    assert!(value["colours"].is_object());
    assert!(!value["colours"].as_object().unwrap().is_empty());
}

#[test]
fn preview_does_not_touch_scheme_state_file() {
    let root = tempdir("state");
    let state_dir = root.join("state/caelestia");
    std::fs::create_dir_all(&state_dir).unwrap();
    let state_file = state_dir.join("scheme.json");
    let before = br#"{"name":"catppuccin","flavour":"mocha","mode":"dark","variant":"tonalspot","colours":{"primary":"7171ac"}}"#;
    std::fs::write(&state_file, before).unwrap();

    let _ = run_caelestia(&root, &["scheme", "preview", "--variant", "monochrome"]);
    let after = std::fs::read(&state_file).unwrap();
    assert_eq!(
        before,
        after.as_slice(),
        "scheme.json must be byte-identical"
    );
}

#[test]
fn preview_does_not_spawn_user_visible_state_tools() {
    let root = tempdir("spawn");
    // Stub PATH with a directory that records every call.
    let fake = root.join("bin");
    std::fs::create_dir_all(&fake).unwrap();
    let trace = root.join("calls.log");
    for tool in ["notify-send", "dconf", "qs", "sh", "caelestia-shell"] {
        let path = fake.join(tool);
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\nprintf '%s %s\\n' \"$0\" \"$*\" >> {}\nexit 0\n",
                trace.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
    }
    // Surface schemes/catppuccin/mocha/dark.txt so the palette is readable.
    // `scheme_data_dir()` resolves to `$XDG_DATA_HOME/caelestia/schemes`.
    let ship = std::env::current_dir()
        .unwrap()
        .join("tests/fixtures/schemes");
    let scheme_dest = root.join("data/caelestia/schemes");
    std::fs::create_dir_all(scheme_dest.parent().unwrap()).unwrap();
    let _ = std::fs::remove_dir_all(&scheme_dest);
    let status = std::process::Command::new("cp")
        .args([
            "-a",
            ship.to_str().unwrap(),
            scheme_dest.parent().unwrap().to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success(), "failed to copy schemes data");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_caelestia"));
    cmd.args(["scheme", "preview", "--variant", "tonalspot"])
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("cfg"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("PATH", fake.as_os_str());
    let out = cmd.output().expect("failed to run caelestia binary");
    assert!(out.status.success(), "preview must succeed: {:?}", out);
    let calls = std::fs::read_to_string(&trace).unwrap_or_default();
    assert!(
        !calls.lines().any(|line| line.starts_with("notify-send")),
        "preview must not spawn notify-send: {calls}"
    );
    assert!(
        !calls.lines().any(|line| line.starts_with("dconf")),
        "preview must not spawn dconf: {calls}"
    );
    assert!(
        !calls.lines().any(|line| line.starts_with("qs")),
        "preview must not shell out to qs: {calls}"
    );
}

#[test]
fn preview_rejects_unknown_variant() {
    let root = tempdir("variant");
    let out = run_caelestia(&root, &["scheme", "preview", "--variant", "totally-fake"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("unknown variant"), "stderr: {stderr}");
}
