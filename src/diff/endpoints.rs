use crate::spec::model::ApiSpec;
use std::collections::HashMap;

/// Normalize a path for comparison: strip a known base path prefix and
/// replace every `{param}` segment with a positional placeholder so that
/// `/repos/{owner}/{repo}` and `/repos/{user}/{project}` compare equal.
pub fn normalize_path(path: &str, base_path: &str) -> String {
    let stripped = if !base_path.is_empty() && path.starts_with(base_path) {
        &path[base_path.len()..]
    } else {
        path
    };
    stripped
        .split('/')
        .map(|seg| {
            if seg.starts_with('{') && seg.ends_with('}') {
                "{}"
            } else {
                seg
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchVia {
    /// HTTP method + normalized path matched exactly in both specs.
    Path,
    /// No path match, but `operationId` matched exactly (case-insensitive).
    OperationId,
}

#[derive(Debug, Clone)]
pub enum MatchOutcome {
    Matched { target_idx: usize, via: MatchVia },
    /// The normalized path matched in both specs, but the HTTP method
    /// differs -- deliberately NOT auto-mapped; the user must confirm.
    MethodMismatch { target_idx: usize, target_method: String },
    Unmatched,
}

#[derive(Debug, Clone)]
pub struct EndpointMatch {
    pub source_idx: usize,
    pub outcome: MatchOutcome,
}

/// Deterministic endpoint matching: exact method+path match first, then
/// exact (case-insensitive) `operationId` match, otherwise unmatched.
/// No scoring, no fuzzy path similarity, no "close enough" heuristics --
/// every match this produces is something a diff-reader could verify by
/// eye in the two spec files.
pub fn match_endpoints(source: &ApiSpec, target: &ApiSpec) -> Vec<EndpointMatch> {
    // (method, normalized_path) -> target index, for exact path matches.
    let mut target_by_path: HashMap<(String, String), usize> = HashMap::new();
    // normalized_path -> Vec<target index>, ignoring method, to detect
    // same-path-different-method cases.
    let mut target_paths_any_method: HashMap<String, Vec<usize>> = HashMap::new();
    // lowercased operationId -> target index.
    let mut target_by_op_id: HashMap<String, usize> = HashMap::new();

    for (ti, t) in target.endpoints.iter().enumerate() {
        let np = normalize_path(&t.path, &target.base_path);
        target_by_path.insert((t.method.as_str().to_string(), np.clone()), ti);
        target_paths_any_method.entry(np).or_default().push(ti);
        if let Some(op_id) = &t.operation.operation_id {
            target_by_op_id.entry(op_id.to_ascii_lowercase()).or_insert(ti);
        }
    }

    source
        .endpoints
        .iter()
        .enumerate()
        .map(|(si, s)| {
            let np = normalize_path(&s.path, &source.base_path);

            if let Some(&ti) = target_by_path.get(&(s.method.as_str().to_string(), np.clone())) {
                return EndpointMatch {
                    source_idx: si,
                    outcome: MatchOutcome::Matched {
                        target_idx: ti,
                        via: MatchVia::Path,
                    },
                };
            }

            if let Some(op_id) = &s.operation.operation_id {
                if let Some(&ti) = target_by_op_id.get(&op_id.to_ascii_lowercase()) {
                    return EndpointMatch {
                        source_idx: si,
                        outcome: MatchOutcome::Matched {
                            target_idx: ti,
                            via: MatchVia::OperationId,
                        },
                    };
                }
            }

            if let Some(candidates) = target_paths_any_method.get(&np) {
                if let Some(&ti) = candidates.first() {
                    return EndpointMatch {
                        source_idx: si,
                        outcome: MatchOutcome::MethodMismatch {
                            target_idx: ti,
                            target_method: target.endpoints[ti].method.as_str().to_string(),
                        },
                    };
                }
            }

            EndpointMatch {
                source_idx: si,
                outcome: MatchOutcome::Unmatched,
            }
        })
        .collect()
}
