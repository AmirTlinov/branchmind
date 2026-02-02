#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && (m.permissions().mode() & 0o111 != 0))
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.is_file())
        .unwrap_or(false)
}

fn path_contains_separator(cmd: &str) -> bool {
    cmd.contains(std::path::MAIN_SEPARATOR) || cmd.contains('/')
}

pub(crate) fn find_executable_in_path(name: &str) -> Option<String> {
    if name.trim().is_empty() {
        return None;
    }
    let path_var = std::env::var_os("PATH")?;
    let dirs = std::env::split_paths(&path_var).collect::<Vec<_>>();
    find_executable_in_dirs(name, &dirs)
}

pub(crate) fn find_executable_in_dirs(name: &str, dirs: &[PathBuf]) -> Option<String> {
    if name.trim().is_empty() {
        return None;
    }
    for dir in dirs {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}

pub(crate) fn can_resolve_command(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return false;
    }
    if path_contains_separator(trimmed) {
        return is_executable(Path::new(trimmed));
    }
    find_executable_in_path(trimmed).is_some()
}

pub(crate) fn resolve_optional_bin(explicit: Option<String>, default_name: &str) -> Option<String> {
    if let Some(v) = explicit
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        return Some(v);
    }
    find_executable_in_path(default_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(prefix: &str) -> PathBuf {
        let base = std::env::temp_dir();
        let pid = std::process::id();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let dir = base.join(format!("{prefix}_{pid}_{nonce}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn find_executable_in_path_discovers_stub() {
        let dir = temp_dir("bm_runner_bin_detect");
        let stub = dir.join("claude");
        fs::write(&stub, "#!/bin/sh\necho ok\n").expect("write stub");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&stub).expect("meta").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&stub, perms).expect("chmod");
        }

        let found = find_executable_in_dirs("claude", std::slice::from_ref(&dir));
        assert!(found.is_some(), "should find stub claude in dirs");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn can_resolve_command_handles_paths_and_names() {
        let dir = temp_dir("bm_runner_bin_detect2");
        let stub = dir.join("codex");
        fs::write(&stub, "#!/bin/sh\necho ok\n").expect("write stub");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&stub).expect("meta").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&stub, perms).expect("chmod");
        }

        assert!(can_resolve_command(stub.to_string_lossy().as_ref()));
        assert!(
            find_executable_in_dirs("codex", std::slice::from_ref(&dir)).is_some(),
            "name lookup should find stub in dirs"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
