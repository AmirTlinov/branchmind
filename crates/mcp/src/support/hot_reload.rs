#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BinaryIdentity {
    #[cfg(unix)]
    ino: u64,
    len: u64,
    modified_nanos: u128,
}

impl BinaryIdentity {
    fn for_path(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        let modified_nanos = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        Some(Self {
            #[cfg(unix)]
            ino: meta.ino(),
            len: meta.len(),
            modified_nanos,
        })
    }
}

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[derive(Clone)]
pub(crate) struct HotReload {
    enabled: bool,
    state: Option<Arc<HotReloadState>>,
}

struct HotReloadState {
    exec_candidates: Vec<OsString>,
    monitor_path: PathBuf,
    baseline: BinaryIdentity,
    poll_interval: Duration,
    reload_requested: AtomicBool,
}

impl HotReload {
    pub(crate) fn disabled() -> Self {
        Self {
            enabled: false,
            state: None,
        }
    }

    pub(crate) fn start(enabled: bool, poll_interval: Duration) -> Self {
        if !enabled {
            return Self::disabled();
        }

        let exec_candidates = exec_candidates();
        let Some((monitor_path, baseline)) = resolve_monitor_path(&exec_candidates) else {
            return Self::disabled();
        };

        let state = Arc::new(HotReloadState {
            exec_candidates,
            monitor_path,
            baseline,
            poll_interval,
            reload_requested: AtomicBool::new(false),
        });
        spawn_monitor_thread(Arc::clone(&state));

        Self {
            enabled: true,
            state: Some(state),
        }
    }

    /// Stdio safety guard: only `exec` when the caller guarantees that no read-ahead bytes are
    /// sitting in a userspace buffer (e.g. `BufReader::buffer().is_empty()`).
    pub(crate) fn maybe_exec_if_requested_and_safe(
        &self,
        stdin_buffer_empty: bool,
    ) -> std::io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if !stdin_buffer_empty {
            return Ok(());
        }
        let Some(state) = self.state.as_ref() else {
            return Ok(());
        };
        if !state.reload_requested.load(Ordering::SeqCst) {
            return Ok(());
        }

        exec_self(&state.exec_candidates)
    }

    pub(crate) fn maybe_exec_now(&self) -> std::io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let Some(state) = self.state.as_ref() else {
            return Ok(());
        };
        if !state.reload_requested.load(Ordering::SeqCst) {
            return Ok(());
        }
        exec_self(&state.exec_candidates)
    }
}

fn spawn_monitor_thread(state: Arc<HotReloadState>) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(state.poll_interval);
            if state.reload_requested.load(Ordering::SeqCst) {
                break;
            }
            match BinaryIdentity::for_path(&state.monitor_path) {
                Some(current) if current != state.baseline => {
                    state.reload_requested.store(true, Ordering::SeqCst);
                    break;
                }
                None => {
                    // If the binary disappears or becomes unreadable (common in rebuilds where the old
                    // inode becomes unlinked), request reload.
                    state.reload_requested.store(true, Ordering::SeqCst);
                    break;
                }
                Some(_) => {}
            }
        }
    });
}

fn resolve_monitor_path(exec_candidates: &[OsString]) -> Option<(PathBuf, BinaryIdentity)> {
    for cand in exec_candidates {
        let Some(path) = resolve_exec_program(cand) else {
            continue;
        };
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        let Some(id) = BinaryIdentity::for_path(&path) else {
            continue;
        };
        return Some((path, id));
    }
    None
}

fn exec_candidates() -> Vec<OsString> {
    let mut out: Vec<OsString> = Vec::new();

    // Prefer argv[0] first: it's the most stable \"how we were launched\" handle (and survives the
    // `current_exe()` \"(deleted)\" behavior after rebuilds).
    if let Some(argv0) = std::env::args_os().next() {
        out.push(argv0);
    }
    if let Ok(exe) = std::env::current_exe() {
        let exe = exe.into_os_string();
        if !out.iter().any(|v| v == &exe) {
            out.push(exe);
        }
    }
    let path_fallback = OsString::from("bm_mcp");
    if !out.iter().any(|v| v == &path_fallback) {
        out.push(path_fallback);
    }

    out
}

fn resolve_exec_program(program: &OsString) -> Option<PathBuf> {
    let raw = PathBuf::from(program);
    // If argv[0] is a path, use it directly.
    if raw.components().count() > 1 || raw.is_absolute() {
        return Some(raw);
    }
    resolve_in_path(program)
}

fn resolve_in_path(program: &OsString) -> Option<PathBuf> {
    let path_env = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_env) {
        let cand = dir.join(program);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

#[cfg(unix)]
fn exec_self(exec_candidates: &[OsString]) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;

    let args: Vec<OsString> = std::env::args_os().skip(1).collect();

    let mut last_err: Option<std::io::Error> = None;
    for cand in exec_candidates {
        let Some(path) = resolve_exec_program(cand) else {
            continue;
        };
        let mut cmd = std::process::Command::new(path);
        cmd.args(&args);
        // `exec` only returns on error. On success it replaces the current process image.
        last_err = Some(cmd.exec());
    }

    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "hot reload exec failed")
    }))
}

#[cfg(not(unix))]
fn exec_self(_exec_candidates: &[OsString]) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_changes_after_write() {
        let dir = std::env::temp_dir().join(format!("bm_hot_reload_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("bin");
        std::fs::write(&path, b"v1").unwrap();
        let id1 = BinaryIdentity::for_path(&path).expect("id1");

        std::thread::sleep(Duration::from_millis(10));
        std::fs::write(&path, b"v2-longer").unwrap();
        let id2 = BinaryIdentity::for_path(&path).expect("id2");

        assert_ne!(id1, id2);
    }
}
