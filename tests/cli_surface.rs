use std::process::Command;

#[test]
fn version_does_not_require_python_environment() {
    let out = Command::new(env!("CARGO_BIN_EXE_caelestia"))
        .arg("--version")
        .env_clear()
        .output()
        .expect("failed to run caelestia binary");

    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim(),
        "caelestia 1.0.0"
    );
}
