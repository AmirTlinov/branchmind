#![forbid(unsafe_code)]

pub(in crate::store) struct EvidenceCapturedPayloadArgs<'a> {
    pub(in crate::store) task_id: &'a str,
    pub(in crate::store) entity_kind: &'a str,
    pub(in crate::store) entity_id: &'a str,
    pub(in crate::store) path: Option<&'a str>,
    pub(in crate::store) artifacts_count: usize,
    pub(in crate::store) checks_count: usize,
    pub(in crate::store) attachments_count: usize,
    pub(in crate::store) checkpoints: &'a [String],
}

pub(in crate::store) fn build_evidence_captured_payload(
    args: EvidenceCapturedPayloadArgs<'_>,
) -> String {
    let EvidenceCapturedPayloadArgs {
        task_id,
        entity_kind,
        entity_id,
        path,
        artifacts_count,
        checks_count,
        attachments_count,
        checkpoints,
    } = args;
    let mut out = String::new();
    out.push_str("{\"task\":\"");
    out.push_str(task_id);
    out.push_str("\",\"entity_kind\":\"");
    out.push_str(entity_kind);
    out.push_str("\",\"entity_id\":\"");
    out.push_str(entity_id);
    out.push_str("\",\"path\":");
    match path {
        Some(path) => {
            out.push('"');
            out.push_str(path);
            out.push('"');
        }
        None => out.push_str("null"),
    }
    out.push_str(",\"artifacts\":");
    out.push_str(&artifacts_count.to_string());
    out.push_str(",\"checks\":");
    out.push_str(&checks_count.to_string());
    out.push_str(",\"attachments\":");
    out.push_str(&attachments_count.to_string());
    out.push_str(",\"checkpoints\":[");
    for (idx, cp) in checkpoints.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(cp);
        out.push('"');
    }
    out.push(']');
    out.push('}');
    out
}

pub(in crate::store) struct EvidenceMirrorMetaTxArgs<'a> {
    pub(in crate::store) task_id: &'a str,
    pub(in crate::store) entity_kind: &'a str,
    pub(in crate::store) entity_id: &'a str,
    pub(in crate::store) path: Option<&'a str>,
    pub(in crate::store) artifacts_count: usize,
    pub(in crate::store) checks_count: usize,
    pub(in crate::store) attachments_count: usize,
    pub(in crate::store) event_id: &'a str,
    pub(in crate::store) checkpoints: &'a [String],
}

pub(in crate::store) fn build_evidence_mirror_meta_json(
    args: EvidenceMirrorMetaTxArgs<'_>,
) -> String {
    let EvidenceMirrorMetaTxArgs {
        task_id,
        entity_kind,
        entity_id,
        path,
        artifacts_count,
        checks_count,
        attachments_count,
        event_id,
        checkpoints,
    } = args;

    let mut out = String::new();
    out.push_str("{\"source\":\"tasks_evidence\",\"task_id\":\"");
    out.push_str(task_id);
    out.push_str("\",\"entity_kind\":\"");
    out.push_str(entity_kind);
    out.push_str("\",\"entity_id\":\"");
    out.push_str(entity_id);
    out.push_str("\",\"path\":");
    match path {
        Some(path) => {
            out.push('"');
            out.push_str(path);
            out.push('"');
        }
        None => out.push_str("null"),
    }
    out.push_str(",\"artifacts\":");
    out.push_str(&artifacts_count.to_string());
    out.push_str(",\"checks\":");
    out.push_str(&checks_count.to_string());
    out.push_str(",\"attachments\":");
    out.push_str(&attachments_count.to_string());
    out.push_str(",\"checkpoints\":[");
    for (idx, cp) in checkpoints.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(cp);
        out.push('"');
    }
    out.push(']');
    out.push_str(",\"event_id\":\"");
    out.push_str(event_id);
    out.push_str("\"}");
    out
}
