use std::process::Command;

#[test]
fn record_outside_cargo_project_prints_friendly_error() {
    let dir = tempfile::tempdir().expect("tempdir");

    let output = Command::new(env!("CARGO_BIN_EXE_cargo-chronoscope"))
        .arg("record")
        .current_dir(dir.path())
        .output()
        .expect("run cargo-chronoscope");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no Cargo.toml in"));
    assert!(stderr.contains("cargo-chronoscope must be run from inside a Rust project"));
    assert!(stderr.contains("cargo-chronoscope record"));
}
