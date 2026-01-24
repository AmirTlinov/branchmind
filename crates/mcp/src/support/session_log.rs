#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(crate) struct SessionLog {
    path: PathBuf,
    start_rfc3339: String,
    pid: u32,
    build: String,
    cwd: String,
    args: Vec<String>,
    mode: Option<String>,
    first_line: Option<String>,
    last_method: Option<String>,
    last_error: Option<String>,
    exit: Option<String>,
}

impl SessionLog {
    pub(crate) fn new(storage_dir: &Path) -> Self {
        // Flagship stability: in `--shared` mode the stdio proxy and the daemon are separate
        // processes that both write a session log. If they share the same file, the daemon can
        // overwrite the proxy log right before a proxy-side transport failure, making diagnosis
        // impossible. Keep the primary file as the proxy/client-facing record; write the daemon
        // record to a dedicated file.
        let args = std::env::args().collect::<Vec<_>>();
        let is_daemon = args.iter().any(|arg| arg.as_str() == "--daemon");
        let path = if is_daemon {
            storage_dir.join("branchmind_mcp_last_session_daemon.txt")
        } else {
            storage_dir.join("branchmind_mcp_last_session.txt")
        };
        let cwd = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .to_string_lossy()
            .to_string();
        let this = Self {
            path,
            start_rfc3339: crate::ts_ms_to_rfc3339(crate::now_ms_i64()),
            pid: std::process::id(),
            build: crate::build_fingerprint(),
            cwd,
            args,
            mode: None,
            first_line: None,
            last_method: None,
            last_error: None,
            exit: None,
        };
        this.flush();
        this
    }

    pub(crate) fn note_mode(&mut self, mode: &str, first_line: &str) {
        self.mode = Some(mode.to_string());
        self.first_line = Some(truncate(first_line.trim_end(), 240));
        self.flush();
    }

    pub(crate) fn note_method(&mut self, method: &str) {
        let method = method.trim();
        if method.is_empty() {
            return;
        }
        self.last_method = Some(truncate(method, 96));
        self.flush();
    }

    pub(crate) fn note_error(&mut self, error: &str) {
        let error = error.trim();
        if error.is_empty() {
            return;
        }
        self.last_error = Some(truncate(error, 300));
        self.flush();
    }

    pub(crate) fn note_exit(&mut self, reason: &str) {
        self.exit = Some(truncate(reason.trim(), 120));
        self.flush();
    }

    fn flush(&self) {
        let Some(dir) = self.path.parent() else {
            return;
        };
        let _ = std::fs::create_dir_all(dir);

        let mut out = String::new();
        push_kv(&mut out, "ts_start", &self.start_rfc3339);
        push_kv(&mut out, "pid", &self.pid.to_string());
        push_kv(&mut out, "build", &self.build);
        push_kv(&mut out, "cwd", &self.cwd);
        push_kv(&mut out, "args", &format!("{:?}", self.args));
        if let Some(mode) = &self.mode {
            push_kv(&mut out, "mode", mode);
        }
        if let Some(line) = &self.first_line {
            push_kv(&mut out, "first_line", line);
        }
        if let Some(method) = &self.last_method {
            push_kv(&mut out, "last_method", method);
        }
        if let Some(err) = &self.last_error {
            push_kv(&mut out, "last_error", err);
        }
        if let Some(exit) = &self.exit {
            push_kv(&mut out, "exit", exit);
        }

        let _ = std::fs::write(&self.path, out);
    }
}

fn push_kv(out: &mut String, key: &str, value: &str) {
    use std::fmt::Write as _;
    let _ = writeln!(out, "{key}={value}");
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in value.chars().enumerate() {
        if idx >= max_chars {
            break;
        }
        out.push(ch);
    }
    out
}
