//! Parses OpenAPI 2.0 (Swagger) and OpenAPI 3.x documents (JSON or YAML)
//! into the normalized [`ApiSpec`] model.
//!
//! We intentionally work off raw `serde_json::Value` rather than a typed
//! OpenAPI crate: Swagger 2.0 and OpenAPI 3.x have different enough shapes
//! (`definitions` vs `components.schemas`, `basePath` vs `servers`,
//! `type: file` params, etc.) that hand-rolling the small subset we care
//! about (paths, operations, parameters, schemas) is simpler than bridging
//! two different typed representations.

use crate::error::{Result, ShotgunError};
use crate::spec::model::*;
use serde_json::Value;
use std::collections::BTreeMap;

pub fn parse_spec_str(path: &str, contents: &str) -> Result<ApiSpec> {
    let value: Value = if path.ends_with(".yaml") || path.ends_with(".yml") {
        serde_yaml::from_str(contents).map_err(|e| ShotgunError::SpecParse {
            path: path.to_string(),
            source: e.into(),
        })?
    } else {
        // Try JSON first, fall back to YAML (YAML is a superset for our purposes).
        serde_json::from_str(contents)
            .or_else(|_| serde_yaml::from_str(contents))
            .map_err(|e| ShotgunError::SpecParse {
                path: path.to_string(),
                source: e.into(),
            })?
    };
    parse_spec_value(path, &value)
}

pub fn parse_spec_file(path: &str) -> Result<ApiSpec> {
    let contents = std::fs::read_to_string(path).map_err(|e| ShotgunError::SpecRead {
        path: path.to_string(),
        source: e,
    })?;
    parse_spec_str(path, &contents)
}

fn parse_spec_value(path: &str, value: &Value) -> Result<ApiSpec> {
    if value.get("openapi").and_then(|v| v.as_str()).is_some() {
        Ok(parse_openapi3(value))
    } else if value.get("swagger").and_then(|v| v.as_str()) == Some("2.0") {
        Ok(parse_swagger2(value))
    } else {
        Err(ShotgunError::UnsupportedVersion(format!(
            "could not detect OpenAPI/Swagger version for {path}"
        )))
    }
}

// ---------------------------------------------------------------------
// Swagger 2.0
// ---------------------------------------------------------------------

fn parse_swagger2(doc: &Value) -> ApiSpec {
    let title = doc
        .pointer("/info/title")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let version = doc
        .pointer("/info/version")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let base_path = doc
        .get("basePath")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let definitions = doc.get("definitions").and_then(|v| v.as_object());
    let schemas = definitions
        .map(|defs| build_schema_registry(defs, resolve_swagger2_ref))
        .unwrap_or_default();

    let mut endpoints = Vec::new();
    if let Some(paths) = doc.get("paths").and_then(|v| v.as_object()) {
        for (path, item) in paths {
            let Some(item) = item.as_object() else {
                continue;
            };
            for (method_str, op) in item {
                let Some(method) = HttpMethod::parse(method_str) else {
                    continue;
                };
                endpoints.push(Endpoint {
                    path: path.clone(),
                    method,
                    operation: parse_swagger2_operation(op, doc),
                });
            }
        }
    }

    ApiSpec {
        title,
        version,
        base_path,
        endpoints,
        schemas,
    }
}

fn resolve_swagger2_ref(r: &str) -> Option<String> {
    r.strip_prefix("#/definitions/").map(|s| s.to_string())
}

fn parse_swagger2_operation(op: &Value, doc: &Value) -> Operation {
    let operation_id = op
        .get("operationId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let summary = op
        .get("summary")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut parameters = Vec::new();
    let mut request_body = None;

    if let Some(params) = op.get("parameters").and_then(|v| v.as_array()) {
        for p in params {
            let loc = p.get("in").and_then(|v| v.as_str()).unwrap_or("");
            let name = p
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            match loc {
                "body" => {
                    let schema = p.get("schema");
                    request_body = schema.map(|s| body_from_swagger2_schema(s));
                }
                "path" | "query" | "header" => {
                    let location = match loc {
                        "path" => ParamLocation::Path,
                        "query" => ParamLocation::Query,
                        _ => ParamLocation::Header,
                    };
                    let ty = p
                        .get("type")
                        .and_then(|v| v.as_str())
                        .map(field_type_from_str)
                        .unwrap_or(FieldType::Unknown);
                    let required = p
                        .get("required")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    parameters.push(Parameter {
                        name,
                        location,
                        required,
                        ty,
                    });
                }
                _ => {}
            }
        }
    }

    let response_body = op
        .get("responses")
        .and_then(|responses| find_success_response(responses))
        .and_then(|resp| resolve_swagger2_response(resp, doc))
        .and_then(|resp| resp.get("schema"))
        .map(|s| body_from_swagger2_schema(s));

    Operation {
        operation_id,
        summary,
        parameters,
        request_body,
        response_body,
    }
}

fn body_from_swagger2_schema(schema: &Value) -> Body {
    if let Some(r) = schema.get("$ref").and_then(|v| v.as_str()) {
        return Body {
            fields: vec![],
            schema_ref: resolve_swagger2_ref(r),
            is_array: false,
        };
    }
    if schema.get("type").and_then(|v| v.as_str()) == Some("array") {
        if let Some(items) = schema.get("items") {
            if let Some(r) = items.get("$ref").and_then(|v| v.as_str()) {
                return Body {
                    fields: vec![],
                    schema_ref: resolve_swagger2_ref(r),
                    is_array: true,
                };
            }
            let fields = fields_from_object_schema(items);
            return Body {
                fields,
                schema_ref: None,
                is_array: true,
            };
        }
    }
    Body {
        fields: fields_from_object_schema(schema),
        schema_ref: None,
        is_array: false,
    }
}

// ---------------------------------------------------------------------
// OpenAPI 3.x
// ---------------------------------------------------------------------

fn parse_openapi3(doc: &Value) -> ApiSpec {
    let title = doc
        .pointer("/info/title")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let version = doc
        .pointer("/info/version")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let base_path = doc
        .pointer("/servers/0/url")
        .and_then(|v| v.as_str())
        .and_then(|url| url::Url::parse(url).ok().map(|u| u.path().to_string()))
        .unwrap_or_default();
    let base_path = if base_path == "/" {
        String::new()
    } else {
        base_path
    };

    let definitions = doc.pointer("/components/schemas").and_then(|v| v.as_object());
    let schemas = definitions
        .map(|defs| build_schema_registry(defs, resolve_openapi3_ref))
        .unwrap_or_default();

    let mut endpoints = Vec::new();
    if let Some(paths) = doc.get("paths").and_then(|v| v.as_object()) {
        for (path, item) in paths {
            let Some(item) = item.as_object() else {
                continue;
            };
            for (method_str, op) in item {
                let Some(method) = HttpMethod::parse(method_str) else {
                    continue;
                };
                endpoints.push(Endpoint {
                    path: path.clone(),
                    method,
                    operation: parse_openapi3_operation(op),
                });
            }
        }
    }

    ApiSpec {
        title,
        version,
        base_path,
        endpoints,
        schemas,
    }
}

fn resolve_openapi3_ref(r: &str) -> Option<String> {
    r.strip_prefix("#/components/schemas/").map(|s| s.to_string())
}

fn parse_openapi3_operation(op: &Value) -> Operation {
    let operation_id = op
        .get("operationId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let summary = op
        .get("summary")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut parameters = Vec::new();
    if let Some(params) = op.get("parameters").and_then(|v| v.as_array()) {
        for p in params {
            let loc = p.get("in").and_then(|v| v.as_str()).unwrap_or("");
            let name = p
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let location = match loc {
                "path" => ParamLocation::Path,
                "query" => ParamLocation::Query,
                "header" => ParamLocation::Header,
                _ => continue,
            };
            let ty = p
                .pointer("/schema/type")
                .and_then(|v| v.as_str())
                .map(field_type_from_str)
                .unwrap_or(FieldType::Unknown);
            let required = p
                .get("required")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            parameters.push(Parameter {
                name,
                location,
                required,
                ty,
            });
        }
    }

    let request_body = op
        .pointer("/requestBody/content/application~1json/schema")
        .map(body_from_openapi3_schema);

    let response_body = find_success_response(op.get("responses").unwrap_or(&Value::Null))
        .and_then(|resp| resp.pointer("/content/application~1json/schema"))
        .map(body_from_openapi3_schema);

    Operation {
        operation_id,
        summary,
        parameters,
        request_body,
        response_body,
    }
}

fn body_from_openapi3_schema(schema: &Value) -> Body {
    if let Some(r) = schema.get("$ref").and_then(|v| v.as_str()) {
        return Body {
            fields: vec![],
            schema_ref: resolve_openapi3_ref(r),
            is_array: false,
        };
    }
    if schema.get("type").and_then(|v| v.as_str()) == Some("array") {
        if let Some(items) = schema.get("items") {
            if let Some(r) = items.get("$ref").and_then(|v| v.as_str()) {
                return Body {
                    fields: vec![],
                    schema_ref: resolve_openapi3_ref(r),
                    is_array: true,
                };
            }
            let fields = fields_from_object_schema(items);
            return Body {
                fields,
                schema_ref: None,
                is_array: true,
            };
        }
    }
    Body {
        fields: fields_from_object_schema(schema),
        schema_ref: None,
        is_array: false,
    }
}

// ---------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------

/// Swagger 2.0 allows a per-status response entry to itself be a
/// `$ref: "#/responses/Name"` into the document's top-level `responses` map
/// (rather than inlining `schema` directly) -- Forgejo's spec does this for
/// nearly every endpoint. Follow that one level of indirection so `schema`
/// lookups see the real response object.
fn resolve_swagger2_response<'a>(resp: &'a Value, doc: &'a Value) -> Option<&'a Value> {
    if let Some(r) = resp.get("$ref").and_then(|v| v.as_str()) {
        let name = r.strip_prefix("#/responses/")?;
        return doc.pointer("/responses").and_then(|v| v.get(name));
    }
    Some(resp)
}

fn find_success_response(responses: &Value) -> Option<&Value> {
    let obj = responses.as_object()?;
    for code in ["200", "201", "202", "203", "204"] {
        if let Some(r) = obj.get(code) {
            return Some(r);
        }
    }
    // Fall back to the first 2xx-looking key.
    obj.iter()
        .find(|(k, _)| k.starts_with('2'))
        .map(|(_, v)| v)
}

fn field_type_from_str(s: &str) -> FieldType {
    match s {
        "string" => FieldType::String,
        "integer" => FieldType::Integer,
        "number" => FieldType::Number,
        "boolean" => FieldType::Boolean,
        "array" => FieldType::Array,
        "object" => FieldType::Object,
        _ => FieldType::Unknown,
    }
}

/// Extract flat fields from an inline object schema (used for both v2 and
/// v3, since `properties`/`type`/`items`/`$ref` are shaped the same way).
fn fields_from_object_schema(schema: &Value) -> Vec<Field> {
    let Some(props) = schema.get("properties").and_then(|v| v.as_object()) else {
        return vec![];
    };
    props
        .iter()
        .map(|(name, prop)| field_from_property(name, prop))
        .collect()
}

fn field_from_property(name: &str, prop: &Value) -> Field {
    if let Some(r) = prop.get("$ref").and_then(|v| v.as_str()) {
        let schema_ref = resolve_any_ref(r);
        return Field {
            name: name.to_string(),
            ty: FieldType::Object,
            schema_ref,
            item_ty: None,
            item_schema_ref: None,
        };
    }

    let ty = prop
        .get("type")
        .and_then(|v| v.as_str())
        .map(field_type_from_str)
        .unwrap_or(FieldType::Unknown);

    if ty == FieldType::Array {
        if let Some(items) = prop.get("items") {
            if let Some(r) = items.get("$ref").and_then(|v| v.as_str()) {
                return Field {
                    name: name.to_string(),
                    ty,
                    schema_ref: None,
                    item_ty: Some(FieldType::Object),
                    item_schema_ref: resolve_any_ref(r),
                };
            }
            let item_ty = items
                .get("type")
                .and_then(|v| v.as_str())
                .map(field_type_from_str);
            return Field {
                name: name.to_string(),
                ty,
                schema_ref: None,
                item_ty,
                item_schema_ref: None,
            };
        }
    }

    Field {
        name: name.to_string(),
        ty,
        schema_ref: None,
        item_ty: None,
        item_schema_ref: None,
    }
}

fn resolve_any_ref(r: &str) -> Option<String> {
    resolve_openapi3_ref(r).or_else(|| resolve_swagger2_ref(r))
}

fn build_schema_registry(
    defs: &serde_json::Map<String, Value>,
    _resolve_ref: impl Fn(&str) -> Option<String>,
) -> BTreeMap<String, Schema> {
    let mut out = BTreeMap::new();
    for (name, schema) in defs {
        let fields = fields_from_object_schema(schema);
        out.insert(
            name.clone(),
            Schema {
                name: name.clone(),
                fields,
            },
        );
    }
    out
}
