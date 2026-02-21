#![forbid(unsafe_code)]

#[cfg(unix)]
#[test]
fn shared_reset_cli_prints_single_json_report() {
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!(
        "bm_mcp_shared_reset_smoke_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");

    let output = Command::new(env!("CARGO_BIN_EXE_bm_mcp"))
        .arg("--shared-reset")
        .arg("--storage-dir")
        .arg(&dir)
        .output()
        .expect("run bm_mcp --shared-reset");

    assert!(
        output.status.success(),
        "shared-reset must exit 0; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find(|l| !l.trim().is_empty())
        .expect("shared-reset must print one json line");
    let value: serde_json::Value = serde_json::from_str(line).expect("stdout json");
    assert_eq!(value.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        value.get("mode").and_then(|v| v.as_str()),
        Some("shared-reset")
    );

    let _ = std::fs::remove_dir_all(dir);
}
