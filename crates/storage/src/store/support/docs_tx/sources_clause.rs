use super::super::super::BranchSource;
use rusqlite::types::Value as SqlValue;

pub(in crate::store) fn append_sources_clause(
    sql: &mut String,
    params: &mut Vec<SqlValue>,
    sources: &[BranchSource],
) {
    sql.push('(');
    for (index, source) in sources.iter().enumerate() {
        if index > 0 {
            sql.push_str(" OR ");
        }
        sql.push_str("(branch=?");
        params.push(SqlValue::Text(source.branch.clone()));
        if let Some(cutoff) = source.cutoff_seq {
            sql.push_str(" AND seq <= ?");
            params.push(SqlValue::Integer(cutoff));
        }
        sql.push(')');
    }
    sql.push(')');
}
