#![forbid(unsafe_code)]

mod support;
use support::*;

use serde_json::{Value, json};

#[cfg(unix)]
mod unix {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    #[test]
    fn acceptance_flagship_ux_shared_daily_smoke() {
        let storage_dir = temp_dir("acceptance_flagship_ux_shared_daily_smoke");
        std::fs::create_dir_all(&storage_dir).expect("create storage dir");
        // Keep unix socket paths short (SUN_LEN is typically ~108 bytes).
        let socket_path = storage_dir.join("bm.sock");

        // Some sandboxed environments disallow unix domain sockets (EPERM). In that case, skip.
        let _ = std::fs::remove_file(&socket_path);
        match UnixListener::bind(&socket_path) {
            Ok(listener) => {
                drop(listener);
                let _ = std::fs::remove_file(&socket_path);
            }
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                let _ = std::fs::remove_dir_all(storage_dir);
                return;
            }
            Err(err) => panic!("unix socket bind preflight failed: {err}"),
        }

        let socket = socket_path.to_string_lossy().to_string();
        let args = vec![
            "--shared",
            "--socket",
            socket.as_str(),
            "--toolset",
            "daily",
            "--workspace",
            "ws_acceptance",
            "--response-verbosity",
            "compact",
        ];

        let mut server = Server::start_with_storage_dir(storage_dir.clone(), &args, true);
        server.initialize_default();

        // 1) Status: should work without workspace in args (uses --workspace default).
        let status = tools_call(&mut server, 1, "status", json!({}));
        let status_text = extract_tool_text_str(&status);
        assert!(
            status_text.starts_with("ready "),
            "status must begin with a ready state line, got:\n{status_text}"
        );
        assert!(
            status_text.contains("workspace=ws_acceptance"),
            "status must include default workspace, got:\n{status_text}"
        );
        assert!(
            status_text.contains("build="),
            "status must include build fingerprint, got:\n{status_text}"
        );

        // 2) Forced error: snapshot without any tasks/focus must return a copy/paste recovery cmd.
        let snap_err = tools_call(
            &mut server,
            2,
            "tasks",
            json!({
                "workspace": "ws_acceptance",
                "op": "call",
                "cmd": "tasks.snapshot",
                "args": { "view": "smart" },
                "fmt": "lines"
            }),
        );
        let snap_err_text = extract_tool_text_str(&snap_err);
        assert!(
            snap_err_text.contains("ERROR:"),
            "expected snapshot error payload, got:\n{snap_err_text}"
        );
        let cmds = extract_bm_command_lines(&snap_err_text);
        assert!(
            !cmds.is_empty(),
            "expected at least one recovery command line, got:\n{snap_err_text}"
        );

        // 3) Execute the first recovery command line as-is (copy/paste validity).
        let (tool, args) = parse_portal_command_line(&cmds[0]);
        let started = tools_call(&mut server, 3, &tool, Value::Object(args));
        let started_text = extract_tool_text_str(&started);
        assert!(
            started_text.contains("focus TASK-"),
            "recovery command must create a focus task, got:\n{started_text}"
        );
        assert!(
            !started_text.contains("ERROR:"),
            "recovery command must succeed, got:\n{started_text}"
        );

        // 4) Snapshot now succeeds (self-heal path is complete).
        let snap_ok = tools_call(
            &mut server,
            4,
            "tasks",
            json!({
                "workspace": "ws_acceptance",
                "op": "call",
                "cmd": "tasks.snapshot",
                "args": { "view": "smart" },
                "fmt": "lines"
            }),
        );
        let snap_ok_text = extract_tool_text_str(&snap_ok);
        assert!(
            !snap_ok_text.contains("ERROR:"),
            "snapshot must succeed after recovery, got:\n{snap_ok_text}"
        );
        assert!(
            snap_ok_text.contains("focus TASK-"),
            "snapshot must include a focus line, got:\n{snap_ok_text}"
        );

        // 5) system.cmd.list: must expose the explicit daemon restart escape hatch.
        let cmd_list = tools_call(
            &mut server,
            5,
            "system",
            json!({
                "workspace": "ws_acceptance",
                "op": "call",
                "cmd": "system.cmd.list",
                "args": { "prefix": "system.", "include_hidden": true, "limit": 50 },
                "fmt": "json"
            }),
        );
        let payload = extract_tool_text(&cmd_list);
        let cmds = payload
            .get("result")
            .and_then(|v| v.get("cmds"))
            .and_then(|v| v.as_array())
            .expect("system.cmd.list result.cmds");
        assert!(
            cmds.iter()
                .any(|v| v.as_str() == Some("system.daemon.restart")),
            "system.cmd.list must include system.daemon.restart, got: {cmds:?}"
        );

        // 6) system.skill profiles: daily/strict/deep/teamlead + research alias → deep.
        for profile in ["daily", "strict", "deep", "teamlead"] {
            let skill = tools_call(
                &mut server,
                20,
                "system",
                json!({
                    "workspace": "ws_acceptance",
                    "op": "call",
                    "cmd": "system.skill",
                    "args": { "profile": profile, "max_chars": 160 },
                    "fmt": "json"
                }),
            );
            let payload = extract_tool_text(&skill);
            let text = payload
                .get("result")
                .and_then(|v| v.as_str())
                .expect("system.skill result must be string");
            assert!(
                text.starts_with(&format!("skill profile={profile} ")),
                "system.skill must identify profile early, got:\n{text}"
            );
        }
        let research = tools_call(
            &mut server,
            21,
            "system",
            json!({
                "workspace": "ws_acceptance",
                "op": "call",
                "cmd": "system.skill",
                "args": { "profile": "research", "max_chars": 160 },
                "fmt": "json"
            }),
        );
        let payload = extract_tool_text(&research);
        let text = payload
            .get("result")
            .and_then(|v| v.as_str())
            .expect("system.skill result must be string");
        assert!(
            text.starts_with("skill profile=deep "),
            "system.skill research alias must resolve to deep, got:\n{text}"
        );

        // 7) Explicit daemon restart (shared-only UX) + self-heal on next forwarded request.
        let restart = tools_call(
            &mut server,
            6,
            "system",
            json!({
                "workspace": "ws_acceptance",
                "op": "call",
                "cmd": "system.daemon.restart",
                "args": {},
                "fmt": "lines"
            }),
        );
        let restart_text = extract_tool_text_str(&restart);
        assert!(
            restart_text.contains("daemon restart requested"),
            "system.daemon.restart must return a clear confirmation, got:\n{restart_text}"
        );

        // Next forwarded call must still work (fresh daemon spawned).
        let snap_after = tools_call(
            &mut server,
            7,
            "tasks",
            json!({
                "workspace": "ws_acceptance",
                "op": "call",
                "cmd": "tasks.snapshot",
                "args": { "view": "smart" },
                "fmt": "lines"
            }),
        );
        let snap_after_text = extract_tool_text_str(&snap_after);
        assert!(
            !snap_after_text.contains("ERROR:"),
            "snapshot must still succeed after daemon restart, got:\n{snap_after_text}"
        );

        // Cleanup (best-effort): shut down the daemon so tests do not leak background processes.
        shutdown_daemon(&socket_path);
    }

    fn tools_call(server: &mut Server, id: i64, name: &str, arguments: Value) -> Value {
        server.request_raw(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }))
    }

    fn shutdown_daemon(socket_path: &PathBuf) {
        // Best-effort: if the daemon never started, don't fail test cleanup.
        let Ok(stream) = wait_for_socket(socket_path) else {
            return;
        };
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        send_frame(
            &stream,
            json!({
                "jsonrpc": "2.0",
                "id": 0,
                "method": "branchmind/daemon_shutdown",
                "params": {}
            }),
        );
        let _ = recv_frame(&mut reader);
    }

    fn send_frame(stream: &UnixStream, value: serde_json::Value) {
        let body = serde_json::to_vec(&value).expect("serialize request");
        let mut writer = stream;
        write!(writer, "Content-Length: {}\r\n\r\n", body.len()).expect("write header");
        writer.write_all(&body).expect("write body");
        writer.flush().expect("flush request");
    }

    fn recv_frame(reader: &mut BufReader<UnixStream>) -> serde_json::Value {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            let read = reader.read_line(&mut line).expect("read header line");
            assert!(read > 0, "unexpected EOF reading response headers");
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some((key, value)) = trimmed.split_once(':')
                && key.trim().eq_ignore_ascii_case("content-length")
            {
                content_length = Some(value.trim().parse().expect("content length"));
            }
        }
        let len = content_length.expect("missing content length");
        let mut body = vec![0u8; len];
        reader.read_exact(&mut body).expect("read response body");
        serde_json::from_slice(&body).expect("parse response json")
    }

    fn wait_for_socket(path: &PathBuf) -> Result<UnixStream, ()> {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            if let Ok(stream) = UnixStream::connect(path) {
                return Ok(stream);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        Err(())
    }

    fn temp_dir(test_name: &str) -> PathBuf {
        // Prefer a short runtime dir to avoid Unix domain socket SUN_LEN limits on some systems.
        let base = {
            let tmp = PathBuf::from("/tmp");
            if tmp.is_dir() {
                tmp
            } else {
                std::env::temp_dir()
            }
        };
        let pid = std::process::id();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        test_name.hash(&mut hasher);
        let test_hash = hasher.finish();
        base.join(format!("bm_acc_{pid}_{nonce}_{test_hash:x}"))
    }
}
