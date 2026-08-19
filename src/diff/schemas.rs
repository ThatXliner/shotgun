use crate::mapping::types::{FieldMapping, NestedMapping, SchemaMapping};
use crate::spec::model::{ApiSpec, Body, Field, FieldType};
use std::collections::{BTreeSet, VecDeque};

/// Diff two flat field lists (already resolved from a body or schema) and
/// produce the rules needed to translate target -> source, deterministically:
///
/// - same name + compatible type -> no entry needed (auto-mapped at runtime
///   because the key is identical on both sides)
/// - same name + incompatible type -> flagged in `type_conflicts`, left
///   untouched (no guessed rename, no silent coercion)
/// - field in source only -> `defaults`
/// - field in target only -> left for `unmapped_field_behavior` to handle
///
/// There is no rename inference here. If two fields are semantically the
/// same thing under different names, a human adds that rename by hand --
/// Shotgun won't guess, because a wrong guess that looks confident is worse
/// than an honest gap.
pub struct FieldDiffResult {
    pub mapping: FieldMapping,
    pub nested_refs: Vec<(String, Option<String>, Option<String>)>, // (source field path, source schema ref, target schema ref)
}

pub fn diff_fields(source: &[Field], target: &[Field]) -> FieldDiffResult {
    let mut mapping = FieldMapping::default();
    let mut nested_refs = Vec::new();

    let target_by_name: std::collections::BTreeMap<&str, &Field> =
        target.iter().map(|f| (f.name.as_str(), f)).collect();
    let mut seen_target_names: BTreeSet<&str> = BTreeSet::new();

    for sf in source {
        match target_by_name.get(sf.name.as_str()) {
            Some(tf) => {
                seen_target_names.insert(sf.name.as_str());
                if sf.ty.compatible(&tf.ty) {
                    queue_nested(&mut nested_refs, &sf.name, sf, tf);
                } else {
                    mapping.type_conflicts.insert(
                        sf.name.clone(),
                        format!("source is {:?}, target is {:?}", sf.ty, tf.ty),
                    );
                }
            }
            None => {
                // Field exists in source but not target at all.
                mapping.defaults.insert(sf.name.clone(), sf.ty.zero_value());
            }
        }
    }

    // Fields present in target but not source: listed in `drops` so a
    // source-API client isn't shown upstream-specific fields it doesn't
    // expect (still subject to `unmapped_field_behavior` at runtime).
    let mut drops: Vec<String> = target
        .iter()
        .filter(|tf| !seen_target_names.contains(tf.name.as_str()))
        .map(|tf| tf.name.clone())
        .collect();
    drops.sort();
    mapping.drops = drops;

    FieldDiffResult { mapping, nested_refs }
}

fn queue_nested(
    out: &mut Vec<(String, Option<String>, Option<String>)>,
    field_path: &str,
    sf: &Field,
    tf: &Field,
) {
    let s_ref = sf.schema_ref.clone().or_else(|| sf.item_schema_ref.clone());
    let t_ref = tf.schema_ref.clone().or_else(|| tf.item_schema_ref.clone());
    if sf.ty == FieldType::Object || tf.ty == FieldType::Object {
        if s_ref.is_some() || t_ref.is_some() {
            out.push((field_path.to_string(), s_ref, t_ref));
        }
    } else if sf.item_ty == Some(FieldType::Object) || tf.item_ty == Some(FieldType::Object) {
        if s_ref.is_some() || t_ref.is_some() {
            out.push((field_path.to_string(), s_ref, t_ref));
        }
    }
}

pub fn body_fields<'a>(spec: &'a ApiSpec, body: &'a Body) -> &'a [Field] {
    if let Some(schema_ref) = &body.schema_ref {
        if let Some(schema) = spec.schemas.get(schema_ref) {
            return &schema.fields;
        }
    }
    &body.fields
}

/// Given a set of nested schema references discovered while diffing
/// endpoint bodies, recursively diff those named schemas and produce
/// reusable `[[schemas]]` entries (deduplicated by source schema name).
pub fn build_schema_registry(
    source: &ApiSpec,
    target: &ApiSpec,
    seed_refs: Vec<(Option<String>, Option<String>)>,
) -> Vec<SchemaMapping> {
    let mut queue: VecDeque<(Option<String>, Option<String>)> = seed_refs.into_iter().collect();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();

    while let Some((s_ref, t_ref)) = queue.pop_front() {
        let Some(s_name) = s_ref else { continue };
        if visited.contains(&s_name) {
            continue;
        }
        visited.insert(s_name.clone());

        let Some(s_schema) = source.schemas.get(&s_name) else {
            continue;
        };
        let t_fields: &[Field] = t_ref
            .as_deref()
            .and_then(|n| target.schemas.get(n))
            .map(|s| s.fields.as_slice())
            .unwrap_or(&[]);

        let diff = diff_fields(&s_schema.fields, t_fields);
        out.push(SchemaMapping {
            name: s_name.clone(),
            edited: false,
            renames: diff.mapping.renames,
            defaults: diff.mapping.defaults,
            drops: diff.mapping.drops,
            type_conflicts: diff.mapping.type_conflicts,
        });

        for (_, sr, tr) in diff.nested_refs {
            queue.push_back((sr, tr));
        }
    }

    out
}

/// Convert a field-diff's nested refs into `[[endpoints.response.nested]]`
/// entries, given the schema name has already been registered.
pub fn nested_mappings_from_refs(refs: &[(String, Option<String>, Option<String>)]) -> Vec<NestedMapping> {
    refs.iter()
        .filter_map(|(path, s_ref, _)| {
            s_ref.as_ref().map(|name| NestedMapping {
                path: path.clone(),
                schema_map: name.clone(),
            })
        })
        .collect()
}
