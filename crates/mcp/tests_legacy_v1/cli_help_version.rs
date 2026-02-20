#![forbid(unsafe_code)]

use std::process::Command;

fn temp_dir(test_name: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dir = base.join(format!("bm_mcp_cli_{test_name}_{pid}_{nonce}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn cli_help_exits_zero_and_does_not_create_repo_store() {
    let exe = env!("CARGO_BIN_EXE_bm_mcp");
    let dir = temp_dir("help");

    let output = Command::new(exe)
        .arg("--help")
        .current_dir(&dir)
        .output()
        .expect("run bm_mcp --help");

    assert!(
        output.status.success(),
        "expected zero exit (stderr={})",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("USAGE:"), "help must include USAGE");
    assert!(
        !dir.join(".agents").exists(),
        "--help should not create repo-local storage dirs"
    );
}

#[test]
fn cli_version_exits_zero_and_includes_pkg_version() {
    let exe = env!("CARGO_BIN_EXE_bm_mcp");
    let output = Command::new(exe)
        .arg("--version")
        .output()
        .expect("run bm_mcp --version");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "version output must include crate version (got={stdout})"
    );
    assert!(
        stdout.contains("build="),
        "version output must include build tag"
    );
}
