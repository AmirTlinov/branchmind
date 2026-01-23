#![forbid(unsafe_code)]

use std::sync::OnceLock;
use std::time::UNIX_EPOCH;

pub(crate) fn build_profile_label() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

pub(crate) fn build_git_sha() -> Option<&'static str> {
    option_env!("BM_GIT_SHA").and_then(|v| {
        let v = v.trim();
        if v.is_empty() { None } else { Some(v) }
    })
}

fn fnv1a_update(mut hash: u64, bytes: &[u8]) -> u64 {
    const FNV_PRIME: u64 = 1099511628211;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn binary_build_tag() -> &'static str {
    static TAG: OnceLock<String> = OnceLock::new();
    TAG.get_or_init(|| {
        // Goal: produce a stable, semver-build-metadata-friendly tag for this exact binary.
        // This is intentionally local-machine scoped and exists to prevent stale shared daemons
        // from surviving local rebuilds where git HEAD doesn't change.
        const FNV_OFFSET: u64 = 14695981039346656037;

        let mut hash = FNV_OFFSET;
        if let Ok(exe) = std::env::current_exe() {
            hash = fnv1a_update(hash, exe.to_string_lossy().as_bytes());
            if let Ok(meta) = std::fs::metadata(&exe) {
                hash = fnv1a_update(hash, &meta.len().to_le_bytes());
                if let Ok(modified) = meta.modified()
                    && let Ok(dur) = modified.duration_since(UNIX_EPOCH)
                {
                    let nanos = dur.as_nanos();
                    let lo = (nanos & 0xffff_ffff_ffff_ffff) as u64;
                    let hi = (nanos >> 64) as u64;
                    hash = fnv1a_update(hash, &lo.to_le_bytes());
                    hash = fnv1a_update(hash, &hi.to_le_bytes());
                }
            }
        }
        format!("bin.{hash:016x}")
    })
    .as_str()
}

pub(crate) fn build_fingerprint() -> String {
    // This is used for daemon/proxy compatibility checks. It is intentionally stable and compact.
    //
    // Semantics:
    // - `version` comes from the MCP server contract versioning (semver-ish).
    // - `git` disambiguates same-version local builds.
    // - `profile` prevents confusing debug/release mismatches when a shared daemon is reused.
    // - `bin` disambiguates local rebuilds even when git HEAD doesn't change (dev workflow).
    let version = crate::SERVER_VERSION;
    let profile = build_profile_label();
    let bin = binary_build_tag();
    match build_git_sha() {
        // Semver build metadata is `+<id>(.<id>)*` where `<id>` is `[0-9A-Za-z-]+`.
        // Keep it parseable and stable: `0.1.0+git.<sha>.<profile>`.
        Some(sha) => format!("{version}+git.{sha}.{profile}.{bin}"),
        None => format!("{version}+{profile}.{bin}"),
    }
}
