use serde_json::Value;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_uninstall"))
}

fn make_executable(path: &Path) {
    fs::write(path, "#!/bin/sh\nexit 0\n").expect("write executable");
    let mut permissions = fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("permissions");
}

fn fixture(name: &str) -> (TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("tempdir");
    let bin = directory.path().join("bin");
    fs::create_dir(&bin).expect("bin");
    let executable = bin.join(name);
    make_executable(&executable);
    (directory, executable)
}

fn run(home: &Path, args: &[&str], input: &str) -> Output {
    let existing_path = std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_owned());
    let path = format!("{}:{existing_path}", home.join("bin").display());
    let mut child = Command::new(binary())
        .args(args)
        .env("HOME", home)
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn uninstall");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("input");
    child.wait_with_output().expect("output")
}

#[test]
fn version_is_the_release_version() {
    let output = Command::new(binary())
        .arg("--version")
        .output()
        .expect("version");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "uninstall 0.19.0"
    );
}

#[test]
fn help_describes_safe_modes_without_removed_flags() {
    let output = Command::new(binary()).arg("--help").output().expect("help");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(text.contains("--self-uninstall"));
    assert!(text.contains("--show-dependencies"));
    assert!(text.contains("--json"));
    assert!(!text.contains("--plan"));
    assert!(!text.contains("--why"));
}

#[test]
fn standalone_result_has_clear_source_and_related_file_note() {
    let (directory, executable) = fixture("rust-uninstall-fixture-show");
    let output = run(directory.path(), &["rust-uninstall-fixture-show"], "\n");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("[Standalone]"), "{text}");
    assert!(text.contains("its original source is unknown"), "{text}");
    assert!(
        text.contains("note: related files cannot be identified automatically"),
        "{text}"
    );
    assert!(
        text.contains("REMOVE rust-uninstall-fixture-show"),
        "{text}"
    );
    assert!(executable.exists());
}

#[test]
fn cancelling_standalone_removal_keeps_the_file() {
    let (directory, executable) = fixture("rust-uninstall-fixture-cancel");
    let output = run(directory.path(), &["rust-uninstall-fixture-cancel"], "no\n");
    assert!(output.status.success());
    assert!(executable.exists());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Cancelled."));
}

#[test]
fn exact_short_typed_confirmation_removes_standalone_file() {
    let (directory, executable) = fixture("rust-uninstall-fixture-remove");
    let output = run(
        directory.path(),
        &["rust-uninstall-fixture-remove"],
        "REMOVE rust-uninstall-fixture-remove\n",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!executable.exists());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Finished."));
}

#[test]
fn selected_detected_data_is_removed_after_the_executable() {
    let (directory, executable) = fixture("rust-uninstall-fixture-data");
    let config_root = directory.path().join(".config");
    let data = config_root.join("rust-uninstall-fixture-data");
    fs::create_dir_all(&data).expect("data");
    fs::write(data.join("settings"), "value").expect("settings");
    let output = run(
        directory.path(),
        &["rust-uninstall-fixture-data"],
        "1\nREMOVE rust-uninstall-fixture-data\n",
    );
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!executable.exists());
    assert!(!data.exists());
}

#[test]
fn unselected_detected_data_is_kept() {
    let (directory, executable) = fixture("rust-uninstall-fixture-keep-data");
    let data = directory
        .path()
        .join(".config/rust-uninstall-fixture-keep-data");
    fs::create_dir_all(&data).expect("data");
    let output = run(
        directory.path(),
        &["rust-uninstall-fixture-keep-data"],
        "\nREMOVE rust-uninstall-fixture-keep-data\n",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!executable.exists());
    assert!(data.exists());
}

#[test]
fn json_mode_is_read_only_and_machine_parseable() {
    let (directory, executable) = fixture("rust-uninstall-fixture-json");
    let output = run(
        directory.path(),
        &["rust-uninstall-fixture-json", "--json"],
        "",
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["results"][0]["backend"], "Standalone");
    assert_eq!(value["results"][0]["preview"]["impact"], "UNKNOWN");
    assert!(executable.exists());
}

#[test]
fn self_uninstall_removes_only_the_invoked_copy() {
    let directory = tempfile::tempdir().expect("tempdir");
    let copied = directory.path().join("uninstall");
    fs::copy(binary(), &copied).expect("copy");
    let mut permissions = fs::metadata(&copied).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&copied, permissions).expect("permissions");
    let mut child = Command::new(&copied)
        .arg("--self-uninstall")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("self uninstall");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"y\n")
        .expect("input");
    let output = child.wait_with_output().expect("output");
    assert!(output.status.success());
    assert!(!copied.exists());
    assert!(binary().exists());
}

#[test]
fn self_uninstall_conflicts_fail_before_changes() {
    let output = Command::new(binary())
        .args(["thing", "--self-uninstall"])
        .output()
        .expect("conflict");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn backend_and_confirmation_are_required_together() {
    let output = Command::new(binary())
        .args(["example", "--backend", "APT"])
        .output()
        .expect("validation");
    assert_eq!(output.status.code(), Some(2));
}
