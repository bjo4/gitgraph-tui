use std::process::Command;

#[test]
fn not_a_repo_exits_with_code_1_and_a_friendly_message() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_gitgraph-tui"))
        .arg(dir.path())
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not a git repository"));
    assert!(!stderr.contains("panicked"), "must fail cleanly, not panic");
}
