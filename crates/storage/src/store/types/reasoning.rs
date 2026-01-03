#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct ReasoningRefRow {
    pub branch: String,
    pub notes_doc: String,
    pub graph_doc: String,
    pub trace_doc: String,
}
