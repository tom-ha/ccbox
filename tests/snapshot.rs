//! Integration test for the `--snapshot` JSON diagnostic mode.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn bin() -> PathBuf {
    // Cargo sets CARGO_BIN_EXE_<name> for binaries declared in Cargo.toml.
    PathBuf::from(env!("CARGO_BIN_EXE_ccbox"))
}

fn fixture() -> String {
    std::fs::read_to_string("tests/fixtures/session-info-example.json").unwrap()
}

fn run_snapshot(width: &str, extra_env: &[(&str, &str)]) -> String {
    let claude_dir = tempfile::tempdir().unwrap();
    let mut cmd = Command::new(bin());
    cmd.arg("--snapshot").arg("--width").arg(width);
    cmd.env("CLAUDE_CONFIG_DIR", claude_dir.path())
        .env("CCBOX_UPDATE_CHECK", "0");
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn ccbox");
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(fixture().as_bytes()).unwrap();
    }
    let out = child.wait_with_output().expect("wait ccbox");
    assert!(out.status.success(), "ccbox exited {:?}", out.status);
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

#[test]
fn snapshot_emits_valid_json() {
    let raw = run_snapshot("140", &[]);
    let v: serde_json::Value = serde_json::from_str(&raw).expect("parse JSON");
    assert!(v.is_object());
}

#[test]
fn snapshot_has_no_ansi_escapes() {
    let raw = run_snapshot("140", &[]);
    assert!(
        !raw.contains("\x1b["),
        "snapshot must not contain ANSI escapes"
    );
}

#[test]
fn snapshot_top_level_keys() {
    let raw = run_snapshot("140", &[]);
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    for key in [
        "session",
        "env",
        "theme",
        "width",
        "layout",
        "composition",
        "computed",
    ] {
        assert!(v.get(key).is_some(), "missing top-level key: {key}");
    }
}

#[test]
fn snapshot_reports_component_visibility() {
    let raw = run_snapshot("140", &[]);
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let body = v["composition"]["body"].as_array().expect("body array");
    let ids: Vec<&str> = body.iter().map(|e| e["id"].as_str().unwrap()).collect();
    for required in [
        "context-row",
        "tokens-cost-row",
        "tasks-row",
        "subagents-row",
        "openspec-row",
        "plugins-skills-row",
    ] {
        assert!(ids.contains(&required), "missing component id: {required}");
    }
}

#[test]
fn snapshot_density_minimal_hides_event_rows() {
    let raw = run_snapshot("140", &[("CCBOX_DENSITY", "minimal")]);
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let body = v["composition"]["body"].as_array().unwrap();
    for entry in body {
        let id = entry["id"].as_str().unwrap();
        if [
            "tasks-row",
            "subagents-row",
            "openspec-row",
            "plugins-skills-row",
        ]
        .contains(&id)
        {
            assert_eq!(
                entry["visible"], false,
                "{id} should be hidden under minimal density"
            );
        }
    }
}

#[test]
fn snapshot_reports_model_name() {
    let raw = run_snapshot("140", &[]);
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["computed"]["model_name"], "Sonnet 4.6");
}

#[test]
fn snapshot_layout_for_wide_width() {
    let raw = run_snapshot("140", &[]);
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["layout"], "wide");
}

#[test]
fn snapshot_env_row_visibility_defaults_to_density() {
    let raw = run_snapshot("140", &[]);
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let rv = &v["env"]["row_visibility"];
    for row in ["tasks", "subagents"] {
        assert_eq!(
            rv[row]["source"], "density",
            "{row}: expected density source when no env/state override",
        );
        // standard density includes both rows.
        assert_eq!(rv[row]["visible"], true, "{row}: standard density visible");
    }
}

#[test]
fn snapshot_env_row_visibility_reports_env_source() {
    let raw = run_snapshot("140", &[("CCBOX_SHOW_TASKS", "0")]);
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let rv = &v["env"]["row_visibility"];
    assert_eq!(rv["tasks"]["source"], "env");
    assert_eq!(rv["tasks"]["visible"], false);
    // Subagents untouched.
    assert_eq!(rv["subagents"]["source"], "density");
}

#[test]
fn snapshot_env_row_visibility_reports_state_file_source() {
    // Set up a tempdir with a state file and point ccbox at it via CLAUDE_CONFIG_DIR.
    use std::io::Write;
    use tempfile::TempDir;
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path()).unwrap();
    let mut f = std::fs::File::create(dir.path().join("ccbox-toggles.json")).unwrap();
    f.write_all(br#"{"show_subagents": false}"#).unwrap();
    let claude_dir = dir.path().to_string_lossy().into_owned();

    let raw = run_snapshot("140", &[("CLAUDE_CONFIG_DIR", claude_dir.as_str())]);
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let rv = &v["env"]["row_visibility"];
    assert_eq!(rv["subagents"]["source"], "state_file");
    assert_eq!(rv["subagents"]["visible"], false);
    assert_eq!(rv["tasks"]["source"], "density");
}

#[test]
fn snapshot_state_file_beats_env_var() {
    use std::io::Write;
    use tempfile::TempDir;
    let dir = TempDir::new().unwrap();
    let mut f = std::fs::File::create(dir.path().join("ccbox-toggles.json")).unwrap();
    f.write_all(br#"{"show_tasks": false}"#).unwrap();
    let claude_dir = dir.path().to_string_lossy().into_owned();

    let raw = run_snapshot(
        "140",
        &[
            ("CLAUDE_CONFIG_DIR", claude_dir.as_str()),
            ("CCBOX_SHOW_TASKS", "1"), // env says show; state file says hide.
        ],
    );
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let rv = &v["env"]["row_visibility"];
    assert_eq!(rv["tasks"]["source"], "state_file");
    assert_eq!(rv["tasks"]["visible"], false);
}
