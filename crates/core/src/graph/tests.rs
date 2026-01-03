use super::*;

#[test]
fn graph_node_id_validation() {
    assert_eq!(
        GraphNodeId::try_new("").unwrap_err(),
        GraphNodeIdError::Empty
    );
    assert_eq!(
        GraphNodeId::try_new("  ").unwrap_err(),
        GraphNodeIdError::Empty
    );
    assert_eq!(
        GraphNodeId::try_new("bad|id").unwrap_err(),
        GraphNodeIdError::ContainsPipe
    );
    assert_eq!(
        GraphNodeId::try_new("bad\u{0007}id").unwrap_err(),
        GraphNodeIdError::ContainsControl
    );
    assert!(GraphNodeId::try_new("CARD-123").is_ok());
}

#[test]
fn graph_rel_validation() {
    assert_eq!(GraphRel::try_new("").unwrap_err(), GraphRelError::Empty);
    assert_eq!(
        GraphRel::try_new("bad|rel").unwrap_err(),
        GraphRelError::ContainsPipe
    );
    assert_eq!(
        GraphRel::try_new("bad\u{0000}rel").unwrap_err(),
        GraphRelError::ContainsControl
    );
    assert!(GraphRel::try_new("supports").is_ok());
}

#[test]
fn conflict_id_validation() {
    assert_eq!(ConflictId::try_new("").unwrap_err(), ConflictIdError::Empty);
    assert_eq!(
        ConflictId::try_new("CONFLICT-xyz").unwrap_err(),
        ConflictIdError::InvalidFormat
    );
    assert!(ConflictId::try_new("CONFLICT-0123456789abcdef0123456789abcdef").is_ok());
}

#[test]
fn normalize_tags_is_deterministic_and_safe() {
    let out = normalize_tags(&[
        " Foo ".to_string(),
        "foo".to_string(),
        "BAR".to_string(),
        "".to_string(),
    ])
    .unwrap();
    assert_eq!(out, vec!["bar".to_string(), "foo".to_string()]);

    assert_eq!(
        normalize_tags(&["bad|tag".to_string()]).unwrap_err(),
        GraphTagError::ContainsPipe
    );
}
