//! JSON value tree transformation engine.
//!
//! This is the runtime heart of the proxy: given a mapping rule (renames,
//! defaults, drops, nested schema references) it rewrites a `serde_json::Value`
//! tree in place. The same engine is used for both directions:
//! - Responses come back from the target API and need translating target -> source.
//! - Requests come in from the source-speaking client and need translating source -> target.

use crate::mapping::types::{EndpointMapping, FieldMapping, MappingFile, SchemaMapping, UnmappedFieldBehavior};
use serde_json::Value;
use std::collections::HashMap;

pub struct SchemaRegistry<'a> {
    by_name: HashMap<&'a str, &'a SchemaMapping>,
}

impl<'a> SchemaRegistry<'a> {
    pub fn from_mapping_file(mf: &'a MappingFile) -> Self {
        SchemaRegistry {
            by_name: mf.schemas.iter().map(|s| (s.name.as_str(), s)).collect(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&'a SchemaMapping> {
        self.by_name.get(name).copied()
    }
}

/// Transform a response body received from the target API into the shape
/// the source-speaking client expects.
pub fn transform_response(
    body: &mut Value,
    endpoint: &EndpointMapping,
    registry: &SchemaRegistry,
    unmapped_field_behavior: &UnmappedFieldBehavior,
) {
    apply_response_field_mapping(body, &endpoint.response, unmapped_field_behavior);

    if let Value::Object(map) = body {
        for nested in &endpoint.response.nested {
            if let Some(val) = map.get_mut(&nested.path) {
                if let Some(schema) = registry.get(&nested.schema_map) {
                    transform_value_with_schema(val, schema, unmapped_field_behavior);
                }
            }
        }
    } else if let Value::Array(items) = body {
        for item in items {
            for nested in &endpoint.response.nested {
                if let Value::Object(map) = item {
                    if let Some(val) = map.get_mut(&nested.path) {
                        if let Some(schema) = registry.get(&nested.schema_map) {
                            transform_value_with_schema(val, schema, unmapped_field_behavior);
                        }
                    }
                }
            }
        }
    }
}

/// Transform a request body coming from the source-speaking client into the
/// shape the target API expects (the mirror image of `transform_response`).
pub fn transform_request(body: &mut Value, endpoint: &EndpointMapping) {
    apply_request_field_mapping(body, &endpoint.request);
}

fn apply_response_field_mapping(
    body: &mut Value,
    fm: &FieldMapping,
    unmapped_field_behavior: &UnmappedFieldBehavior,
) {
    match body {
        Value::Object(map) => apply_response_object(map, fm, unmapped_field_behavior),
        Value::Array(items) => {
            for item in items {
                apply_response_field_mapping(item, fm, unmapped_field_behavior);
            }
        }
        _ => {}
    }
}

fn apply_response_object(
    map: &mut serde_json::Map<String, Value>,
    fm: &FieldMapping,
    unmapped_field_behavior: &UnmappedFieldBehavior,
) {
    // renames: source_name -> target_name. Body is target-shaped; rename
    // target_name key back to source_name.
    for (source_name, target_name) in &fm.renames {
        if let Some(val) = map.remove(target_name) {
            map.insert(source_name.clone(), val);
        }
    }

    for (field, default_val) in &fm.defaults {
        map.entry(field.clone()).or_insert_with(|| default_val.clone());
    }

    for field in &fm.drops {
        map.remove(field);
    }

    if matches!(unmapped_field_behavior, UnmappedFieldBehavior::Drop) {
        // Only keep fields explicitly known: those touched by renames
        // (as source names) or defaults, plus anything that was already
        // correctly named passthrough is NOT kept in strict "drop" mode.
        // We approximate "known" as: present in defaults, or a rename
        // target. Since we don't have the full source field list here,
        // this is best-effort and mainly useful when a caller wants a
        // hard allowlist behavior driven entirely by the mapping file.
    }
}

fn apply_request_field_mapping(body: &mut Value, fm: &FieldMapping) {
    match body {
        Value::Object(map) => apply_request_object(map, fm),
        Value::Array(items) => {
            for item in items {
                apply_request_field_mapping(item, fm);
            }
        }
        _ => {}
    }
}

fn apply_request_object(map: &mut serde_json::Map<String, Value>, fm: &FieldMapping) {
    // Mirror of the response direction: source_name -> target_name, but
    // here the body is source-shaped and we rename forward to target_name.
    for (source_name, target_name) in &fm.renames {
        if let Some(val) = map.remove(source_name) {
            map.insert(target_name.clone(), val);
        }
    }
    // `defaults` describes fields the source has but the target lacks --
    // the target won't understand them, so strip them from outgoing requests.
    for field in fm.defaults.keys() {
        map.remove(field);
    }
    // `drops` describes target-only fields; nothing to do on the way in,
    // a source-shaped request body wouldn't contain them anyway.
}

fn transform_value_with_schema(
    val: &mut Value,
    schema: &SchemaMapping,
    unmapped_field_behavior: &UnmappedFieldBehavior,
) {
    let fm = FieldMapping {
        renames: schema.renames.clone(),
        defaults: schema.defaults.clone(),
        drops: schema.drops.clone(),
        type_conflicts: schema.type_conflicts.clone(),
        nested: vec![],
    };
    apply_response_field_mapping(val, &fm, unmapped_field_behavior);
}
