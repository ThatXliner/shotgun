use shotgun::diff::schemas::diff_fields;
use shotgun::spec::model::{Field, FieldType};

fn field(name: &str, ty: FieldType) -> Field {
    Field {
        name: name.to_string(),
        ty,
        schema_ref: None,
        item_ty: None,
        item_schema_ref: None,
    }
}

#[test]
fn exact_name_matches_produce_no_rename() {
    let source = vec![field("id", FieldType::Integer), field("tag", FieldType::String)];
    let target = vec![field("id", FieldType::Integer), field("tag", FieldType::String)];

    let diff = diff_fields(&source, &target);
    assert!(diff.mapping.renames.is_empty());
    assert!(diff.mapping.defaults.is_empty());
    assert!(diff.mapping.drops.is_empty());
    assert!(diff.mapping.type_conflicts.is_empty());
}

#[test]
fn differently_named_fields_are_never_guessed_as_renames() {
    // Even an obvious-looking synonym pair must NOT be auto-mapped: Shotgun
    // never infers renames, only exact name matches.
    let source = vec![field("created_at", FieldType::String)];
    let target = vec![field("created", FieldType::String)];

    let diff = diff_fields(&source, &target);
    assert!(diff.mapping.renames.is_empty());
    assert!(diff.mapping.defaults.contains_key("created_at"));
    assert_eq!(diff.mapping.drops, vec!["created".to_string()]);
}

#[test]
fn source_only_field_becomes_default() {
    let source = vec![field("node_id", FieldType::String)];
    let target: Vec<Field> = vec![];

    let diff = diff_fields(&source, &target);
    assert_eq!(diff.mapping.defaults.get("node_id"), Some(&serde_json::Value::String(String::new())));
}

#[test]
fn target_only_field_becomes_drop() {
    let source: Vec<Field> = vec![];
    let target = vec![field("internal_tracker", FieldType::String)];

    let diff = diff_fields(&source, &target);
    assert_eq!(diff.mapping.drops, vec!["internal_tracker".to_string()]);
}

#[test]
fn same_name_incompatible_types_flagged_as_conflict_not_dropped_or_defaulted() {
    let source = vec![field("tags", FieldType::Array)];
    let target = vec![field("tags", FieldType::String)];

    let diff = diff_fields(&source, &target);
    // Same name, incompatible type: flagged for human review, not silently
    // defaulted, dropped, or renamed.
    assert!(diff.mapping.type_conflicts.contains_key("tags"));
    assert!(diff.mapping.renames.is_empty());
    assert!(diff.mapping.defaults.is_empty());
    assert!(diff.mapping.drops.is_empty());
}

#[test]
fn compatible_numeric_types_are_not_flagged() {
    let source = vec![field("count", FieldType::Integer)];
    let target = vec![field("count", FieldType::Number)];

    let diff = diff_fields(&source, &target);
    assert!(diff.mapping.type_conflicts.is_empty());
    assert!(diff.mapping.is_empty());
}
