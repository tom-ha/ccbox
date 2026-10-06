use std::io::Write;
use std::process::{Command, Stdio};

use tempfile::tempdir;

#[test]
fn completion_is_recorded_when_waiting_row_is_off() {
    let claude = tempdir().unwrap();
    let home = tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ccbox"))
        .arg("hook")
        .env("CLAUDE_CONFIG_DIR", claude.path())
        .env("HOME", home.path())
        .env_remove("CCBOX_SHOW_WAITING")
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"hook_event_name":"SubagentStop","session_id":"s","agent_id":"a"}"#)
        .unwrap();
    assert!(child.wait().unwrap().success());
    assert!(claude
        .path()
        .join("ccbox-cache/subagents/s/a.done")
        .exists());
    assert!(!claude.path().join("ccbox-cache/waiting").exists());
}

#[test]
fn session_end_replaces_markers_with_the_end_time() {
    let claude = tempdir().unwrap();
    let home = tempdir().unwrap();
    let run = |payload: &[u8]| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ccbox"))
            .arg("hook")
            .env("CLAUDE_CONFIG_DIR", claude.path())
            .env("HOME", home.path())
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(payload).unwrap();
        assert!(child.wait().unwrap().success());
    };
    run(br#"{"hook_event_name":"SubagentStop","session_id":"s","agent_id":"a"}"#);
    run(br#"{"hook_event_name":"SessionEnd","session_id":"s","reason":"prompt_input_exit"}"#);
    assert!(!claude.path().join("ccbox-cache/subagents/s").exists());
    assert!(claude.path().join("ccbox-cache/subagents/s.ended").exists());
}
