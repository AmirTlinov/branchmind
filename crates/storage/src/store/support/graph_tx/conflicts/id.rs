#![forbid(unsafe_code)]

use super::super::types::GraphConflictIdArgs;

pub(super) fn graph_conflict_id(args: GraphConflictIdArgs<'_>) -> String {
    let GraphConflictIdArgs {
        workspace,
        from_branch,
        into_branch,
        doc,
        kind,
        key,
        base_cutoff_seq,
        theirs_seq,
        ours_seq,
    } = args;

    const FNV_OFFSET: u64 = 14695981039346656037;
    const FNV_PRIME: u64 = 1099511628211;

    fn update_str(hash: &mut u64, value: &str) {
        for b in value.as_bytes() {
            *hash ^= *b as u64;
            *hash = hash.wrapping_mul(FNV_PRIME);
        }
        *hash ^= 0xff;
        *hash = hash.wrapping_mul(FNV_PRIME);
    }

    fn update_i64(hash: &mut u64, value: i64) {
        for b in value.to_le_bytes() {
            *hash ^= b as u64;
            *hash = hash.wrapping_mul(FNV_PRIME);
        }
        *hash ^= 0xff;
        *hash = hash.wrapping_mul(FNV_PRIME);
    }

    let mut h1 = FNV_OFFSET;
    let mut h2 = FNV_OFFSET ^ 0x9e3779b97f4a7c15;

    for (hash, offset) in [(&mut h1, 0u8), (&mut h2, 1u8)] {
        update_str(hash, workspace);
        update_str(hash, from_branch);
        update_str(hash, into_branch);
        update_str(hash, doc);
        update_str(hash, kind);
        update_str(hash, key);
        update_i64(hash, base_cutoff_seq);
        update_i64(hash, theirs_seq);
        update_i64(hash, ours_seq);
        *hash ^= offset as u64;
        *hash = hash.wrapping_mul(FNV_PRIME);
    }

    format!("CONFLICT-{h1:016x}{h2:016x}")
}
