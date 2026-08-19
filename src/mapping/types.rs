use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Meta {
    pub source_spec: String,
    pub target_spec: String,
    pub generated_at: String,
    pub shotgun_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnmappedEndpointBehavior {
    Reject,
    Passthrough,
}

impl Default for UnmappedEndpointBehavior {
    fn default() -> Self {
        UnmappedEndpointBehavior::Reject
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnmappedFieldBehavior {
    Drop,
    Passthrough,
    DropUnknown,
}

impl Default for UnmappedFieldBehavior {
    fn default() -> Self {
        UnmappedFieldBehavior::Passthrough
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PaginationSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_style: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rewrite_link_urls: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub param_map: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub target_base_path: String,
    /// Prefix stripped from the incoming request path before matching it
    /// against `[[endpoints]]` sources. Needed when the source-speaking
    /// client itself prepends a fixed prefix that isn't part of the API's
    /// logical path shape -- e.g. the `gh` CLI (and other GitHub Enterprise
    /// Server-aware clients) request `/api/v3/...` for any host that isn't
    /// literally `github.com`, even though the mapping file's endpoints are
    /// written against the plain `github.com` path shape (`/user`, not
    /// `/api/v3/user`).
    #[serde(default)]
    pub source_base_path: String,
    #[serde(default)]
    pub unmapped_endpoint_behavior: UnmappedEndpointBehavior,
    #[serde(default)]
    pub unmapped_field_behavior: UnmappedFieldBehavior,
    #[serde(default, skip_serializing_if = "is_default_pagination")]
    pub pagination: PaginationSettings,
    /// Headers to add to every mapped response that the target API has no
    /// concept of at all (e.g. GitHub's `X-RateLimit-*` headers, which
    /// Forgejo doesn't send). Static values only -- these are synthesized,
    /// not translated from an upstream header, so they're inserted
    /// unconditionally and overwrite any same-named upstream header.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub synthesized_response_headers: BTreeMap<String, String>,
}

fn is_default_pagination(p: &PaginationSettings) -> bool {
    p.source_style.is_none()
        && p.target_style.is_none()
        && !p.rewrite_link_urls
        && p.param_map.is_empty()
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            target_base_path: String::new(),
            source_base_path: String::new(),
            unmapped_endpoint_behavior: UnmappedEndpointBehavior::Reject,
            unmapped_field_behavior: UnmappedFieldBehavior::Passthrough,
            pagination: PaginationSettings::default(),
            synthesized_response_headers: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NestedMapping {
    pub path: String,
    pub schema_map: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FieldMapping {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub renames: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub defaults: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drops: Vec<String>,
    /// Fields with the same name in both source and target, but
    /// incompatible types. Auto-diff can't safely map these -- it leaves
    /// the field untouched at runtime (same key, whatever the upstream
    /// sends) and records why here so a human can decide what to do.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub type_conflicts: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nested: Vec<NestedMapping>,
}

impl FieldMapping {
    pub fn is_empty(&self) -> bool {
        self.renames.is_empty()
            && self.defaults.is_empty()
            && self.drops.is_empty()
            && self.type_conflicts.is_empty()
            && self.nested.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EndpointMapping {
    pub source: String,
    #[serde(default)]
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// True once a human has hand-edited this entry; preserved across `sync`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub edited: bool,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub path_params: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub query_params: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,

    #[serde(default, skip_serializing_if = "FieldMapping::is_empty")]
    pub request: FieldMapping,
    #[serde(default, skip_serializing_if = "FieldMapping::is_empty")]
    pub response: FieldMapping,
}

impl EndpointMapping {
    pub fn method_and_path(source_or_target: &str) -> Option<(String, String)> {
        let mut parts = source_or_target.splitn(2, ' ');
        let method = parts.next()?.to_string();
        let path = parts.next()?.to_string();
        Some((method, path))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemaMapping {
    pub name: String,
    /// True once a human has hand-edited this entry; preserved across `sync`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub edited: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub renames: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub defaults: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drops: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub type_conflicts: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MappingFile {
    #[serde(default)]
    pub meta: Meta,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default, rename = "endpoints")]
    pub endpoints: Vec<EndpointMapping>,
    #[serde(default, rename = "schemas")]
    pub schemas: Vec<SchemaMapping>,
}

impl MappingFile {
    pub fn find_endpoint(&self, method: &str, path: &str) -> Option<&EndpointMapping> {
        self.endpoints
            .iter()
            .find(|e| e.source == format!("{method} {path}"))
    }

    pub fn coverage(&self) -> (usize, usize) {
        let mapped = self.endpoints.iter().filter(|e| !e.target.is_empty()).count();
        (mapped, self.endpoints.len())
    }

    pub fn schema_by_name(&self, name: &str) -> Option<&SchemaMapping> {
        self.schemas.iter().find(|s| s.name == name)
    }
}
