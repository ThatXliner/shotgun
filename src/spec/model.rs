//! Normalized, version-agnostic representation of an OpenAPI/Swagger document.
//!
//! Both OpenAPI 3.x and Swagger 2.0 specs are parsed down into this shape so
//! the rest of Shotgun (diffing, mapping, proxying) never has to think about
//! which spec version it originated from.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HttpMethod {
    Get,
    Put,
    Post,
    Delete,
    Options,
    Head,
    Patch,
    Trace,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Put => "PUT",
            HttpMethod::Post => "POST",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Options => "OPTIONS",
            HttpMethod::Head => "HEAD",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Trace => "TRACE",
        }
    }

    pub fn parse(s: &str) -> Option<HttpMethod> {
        Some(match s.to_ascii_lowercase().as_str() {
            "get" => HttpMethod::Get,
            "put" => HttpMethod::Put,
            "post" => HttpMethod::Post,
            "delete" => HttpMethod::Delete,
            "options" => HttpMethod::Options,
            "head" => HttpMethod::Head,
            "patch" => HttpMethod::Patch,
            "trace" => HttpMethod::Trace,
            _ => return None,
        })
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A simplified type descriptor for a schema/field. We don't need the full
/// JSON Schema power here -- just enough to compare shapes between two specs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    Integer,
    Number,
    Boolean,
    Array,
    Object,
    Unknown,
}

impl FieldType {
    /// Whether two types are "compatible enough" to be considered the same
    /// field without flagging for manual review.
    pub fn compatible(&self, other: &FieldType) -> bool {
        use FieldType::*;
        match (self, other) {
            (a, b) if a == b => true,
            (Integer, Number) | (Number, Integer) => true,
            _ => false,
        }
    }

    /// A reasonable zero value for this type, used when generating `defaults`.
    pub fn zero_value(&self) -> serde_json::Value {
        match self {
            FieldType::String => serde_json::Value::String(String::new()),
            FieldType::Integer | FieldType::Number => serde_json::Value::from(0),
            FieldType::Boolean => serde_json::Value::Bool(false),
            FieldType::Array => serde_json::Value::Array(vec![]),
            FieldType::Object => serde_json::Value::Object(Default::default()),
            // TOML has no null literal, and `defaults` values get written as
            // TOML -- an actual null here would make every mapping file
            // unserializable. Empty string is the least-wrong placeholder.
            FieldType::Unknown => serde_json::Value::String(String::new()),
        }
    }
}

/// A field within an object schema.
#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: FieldType,
    /// If `ty == Object`, name of the referenced schema (for recursive matching).
    pub schema_ref: Option<String>,
    /// If `ty == Array`, the type of the array's items.
    pub item_ty: Option<FieldType>,
    pub item_schema_ref: Option<String>,
}

/// A named, reusable object schema (e.g. "User", "Repository").
#[derive(Debug, Clone, Default)]
pub struct Schema {
    pub name: String,
    pub fields: Vec<Field>,
}

/// A single request/response body shape attached to an operation.
#[derive(Debug, Clone, Default)]
pub struct Body {
    /// Inline fields (when the body isn't a named schema).
    pub fields: Vec<Field>,
    /// Reference to a named schema, if the body is `$ref`'d or a named type.
    pub schema_ref: Option<String>,
    /// True if the body/response is an array of the above shape.
    pub is_array: bool,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub location: ParamLocation,
    pub required: bool,
    pub ty: FieldType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamLocation {
    Path,
    Query,
    Header,
}

#[derive(Debug, Clone, Default)]
pub struct Operation {
    pub operation_id: Option<String>,
    pub summary: Option<String>,
    pub parameters: Vec<Parameter>,
    pub request_body: Option<Body>,
    /// Response body for the "primary" success response (2xx), if any.
    pub response_body: Option<Body>,
}

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub path: String,
    pub method: HttpMethod,
    pub operation: Operation,
}

impl Endpoint {
    pub fn key(&self) -> String {
        format!("{} {}", self.method, self.path)
    }
}

/// The normalized representation of an entire API spec, regardless of
/// whether it started life as Swagger 2.0 or OpenAPI 3.x.
#[derive(Debug, Clone, Default)]
pub struct ApiSpec {
    pub title: String,
    pub version: String,
    /// Base path prefix (e.g. "/api/v1") extracted from `basePath` (v2) or
    /// the first server URL's path component (v3).
    pub base_path: String,
    pub endpoints: Vec<Endpoint>,
    /// Named reusable schemas, keyed by name (from `definitions` in v2 or
    /// `components.schemas` in v3).
    pub schemas: BTreeMap<String, Schema>,
}

impl ApiSpec {
    pub fn endpoint_count(&self) -> usize {
        self.endpoints.len()
    }
}
