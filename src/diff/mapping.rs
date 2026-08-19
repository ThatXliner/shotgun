use crate::diff::endpoints::{match_endpoints, MatchOutcome, MatchVia};
use crate::diff::schemas::{body_fields, build_schema_registry, diff_fields, nested_mappings_from_refs};
use crate::mapping::types::{EndpointMapping, MappingFile, Meta, Settings};
use crate::spec::model::ApiSpec;

pub struct DiffSummary {
    pub total_source_endpoints: usize,
    pub mapped: usize,
    pub method_mismatches: usize,
    pub unmatched: usize,
}

pub fn build_mapping_file(
    source: &ApiSpec,
    target: &ApiSpec,
    source_spec_path: &str,
    target_spec_path: &str,
    generated_at: &str,
) -> (MappingFile, DiffSummary) {
    let matches = match_endpoints(source, target);
    let mut endpoints = Vec::new();
    let mut seed_refs: Vec<(Option<String>, Option<String>)> = Vec::new();
    let mut mapped_count = 0;
    let mut method_mismatches = 0;
    let mut unmatched = 0;

    for m in &matches {
        let s = &source.endpoints[m.source_idx];

        let (target_idx, via) = match &m.outcome {
            MatchOutcome::Matched { target_idx, via } => (*target_idx, via.clone()),
            MatchOutcome::MethodMismatch {
                target_idx,
                target_method,
            } => {
                method_mismatches += 1;
                let t = &target.endpoints[*target_idx];
                endpoints.push(EndpointMapping {
                    source: s.key(),
                    target: String::new(),
                    note: Some(format!(
                        "path matches '{}' in target, but method differs (source {} vs target {}) -- confirm and map manually",
                        t.path, s.method, target_method
                    )),
                    ..Default::default()
                });
                continue;
            }
            MatchOutcome::Unmatched => {
                unmatched += 1;
                endpoints.push(EndpointMapping {
                    source: s.key(),
                    target: String::new(),
                    note: Some("no matching path or operationId found in target spec".to_string()),
                    ..Default::default()
                });
                continue;
            }
        };
        let t = &target.endpoints[target_idx];
        mapped_count += 1;

        // Path parameter renames: positional match between {param} segments.
        let mut path_params = std::collections::BTreeMap::new();
        let s_params: Vec<&str> = s
            .path
            .split('/')
            .filter(|seg| seg.starts_with('{') && seg.ends_with('}'))
            .map(|seg| &seg[1..seg.len() - 1])
            .collect();
        let t_params: Vec<&str> = t
            .path
            .split('/')
            .filter(|seg| seg.starts_with('{') && seg.ends_with('}'))
            .map(|seg| &seg[1..seg.len() - 1])
            .collect();
        for (sp, tp) in s_params.iter().zip(t_params.iter()) {
            if sp != tp {
                path_params.insert(sp.to_string(), tp.to_string());
            }
        }

        let response = s
            .operation
            .response_body
            .as_ref()
            .map(|sb| {
                let s_fields = body_fields(source, sb).to_vec();
                let t_fields = t
                    .operation
                    .response_body
                    .as_ref()
                    .map(|tb| body_fields(target, tb).to_vec())
                    .unwrap_or_default();
                let diff = diff_fields(&s_fields, &t_fields);
                for (_, sr, tr) in &diff.nested_refs {
                    seed_refs.push((sr.clone(), tr.clone()));
                }
                let mut field_mapping = diff.mapping;
                field_mapping.nested = nested_mappings_from_refs(&diff.nested_refs);
                field_mapping
            })
            .unwrap_or_default();

        let request = s
            .operation
            .request_body
            .as_ref()
            .map(|sb| {
                let s_fields = body_fields(source, sb).to_vec();
                let t_fields = t
                    .operation
                    .request_body
                    .as_ref()
                    .map(|tb| body_fields(target, tb).to_vec())
                    .unwrap_or_default();
                diff_fields(&s_fields, &t_fields).mapping
            })
            .unwrap_or_default();

        let note = match via {
            MatchVia::Path => None,
            MatchVia::OperationId => Some(
                "matched by operationId; path differs between source and target -- verify path params below"
                    .to_string(),
            ),
        };

        endpoints.push(EndpointMapping {
            source: s.key(),
            target: t.key(),
            note,
            edited: false,
            path_params,
            query_params: Default::default(),
            headers: Default::default(),
            request,
            response,
        });
    }

    endpoints.sort_by(|a, b| a.source.cmp(&b.source));

    let schemas = build_schema_registry(source, target, seed_refs);

    let mapping = MappingFile {
        meta: Meta {
            source_spec: source_spec_path.to_string(),
            target_spec: target_spec_path.to_string(),
            generated_at: generated_at.to_string(),
            shotgun_version: env!("CARGO_PKG_VERSION").to_string(),
        },
        settings: Settings {
            target_base_path: target.base_path.clone(),
            ..Default::default()
        },
        endpoints,
        schemas,
    };

    let summary = DiffSummary {
        total_source_endpoints: source.endpoint_count(),
        mapped: mapped_count,
        method_mismatches,
        unmatched,
    };

    (mapping, summary)
}
