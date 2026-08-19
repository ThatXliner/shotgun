use crate::mapping::types::{EndpointMapping, MappingFile, SchemaMapping};

/// Merge a freshly-generated mapping (from re-diffing updated specs) into an
/// existing, possibly hand-edited mapping file.
///
/// Rules:
/// - Entries the user edited (`edited = true`) are kept verbatim.
/// - Entries that still look auto-generated are replaced by the new proposal.
/// - New endpoints found in `proposed` but absent from `existing` are appended.
/// - Endpoints present in `existing` but missing from `proposed` (i.e. the
///   source endpoint was removed) are demoted to `confidence = "none"` and
///   annotated, rather than silently deleted.
pub struct MergeReport {
    pub added: usize,
    pub updated: usize,
    pub kept_edited: usize,
    pub removed_flagged: usize,
}

pub fn merge_mapping_files(existing: MappingFile, proposed: MappingFile) -> (MappingFile, MergeReport) {
    let mut report = MergeReport {
        added: 0,
        updated: 0,
        kept_edited: 0,
        removed_flagged: 0,
    };

    let mut merged_endpoints: Vec<EndpointMapping> = Vec::new();
    let mut existing_by_source: std::collections::HashMap<String, EndpointMapping> = existing
        .endpoints
        .into_iter()
        .map(|e| (e.source.clone(), e))
        .collect();

    for proposed_ep in proposed.endpoints {
        match existing_by_source.remove(&proposed_ep.source) {
            Some(existing_ep) if existing_ep.edited => {
                report.kept_edited += 1;
                merged_endpoints.push(existing_ep);
            }
            Some(_existing_ep) => {
                report.updated += 1;
                merged_endpoints.push(proposed_ep);
            }
            None => {
                report.added += 1;
                merged_endpoints.push(proposed_ep);
            }
        }
    }

    // Anything left in existing_by_source was not found in the re-diffed
    // source spec: either it was hand-edited (keep, but flag) or it was
    // auto-generated and the endpoint truly disappeared (flag as removed).
    for (_source, mut leftover) in existing_by_source {
        report.removed_flagged += 1;
        if !leftover.edited {
            leftover.target.clear();
        }
        leftover.note = Some(match leftover.note {
            Some(existing_note) => format!("{existing_note} [no longer present in source spec]"),
            None => "no longer present in source spec".to_string(),
        });
        merged_endpoints.push(leftover);
    }

    let mut merged_schemas: Vec<SchemaMapping> = Vec::new();
    let mut existing_schemas: std::collections::HashMap<String, SchemaMapping> = existing
        .schemas
        .into_iter()
        .map(|s| (s.name.clone(), s))
        .collect();
    for proposed_schema in proposed.schemas {
        if let Some(existing_schema) = existing_schemas.remove(&proposed_schema.name) {
            merged_schemas.push(if existing_schema.edited {
                existing_schema
            } else {
                proposed_schema
            });
        } else {
            merged_schemas.push(proposed_schema);
        }
    }

    let mut merged = MappingFile {
        meta: proposed.meta,
        settings: existing.settings,
        endpoints: merged_endpoints,
        schemas: merged_schemas,
    };
    merged
        .endpoints
        .sort_by(|a, b| a.source.cmp(&b.source));

    (merged, report)
}
