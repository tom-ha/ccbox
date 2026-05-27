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
    let mut cmd = Command::new(bin());
    cmd.arg("--snapshot").arg("--width").arg(width);
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
