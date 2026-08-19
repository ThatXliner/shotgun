//! Pagination handling: query-param name translation and `Link` header URL
//! rewriting so paginated responses keep pointing back through the proxy
//! instead of leaking the upstream target URL to source-speaking clients.

use crate::mapping::types::PaginationSettings;
use http::HeaderMap;
use url::Url;

/// Rewrite source-side pagination query params to their target-side names
/// (e.g. `per_page` -> `limit`) before forwarding a request upstream.
pub fn translate_query_params(pairs: &mut Vec<(String, String)>, settings: &PaginationSettings) {
    for (key, _) in pairs.iter_mut() {
        if let Some(target_name) = settings.param_map.get(key.as_str()) {
            *key = target_name.clone();
        }
    }
}

/// Rewrite target-side pagination param names back to source names in the
/// reverse direction (used if we ever need to reflect param names back to
/// the client, e.g. in generated `Link` headers referencing our own proxy).
pub fn translate_query_params_reverse(pairs: &mut Vec<(String, String)>, settings: &PaginationSettings) {
    let reverse: std::collections::HashMap<&str, &str> = settings
        .param_map
        .iter()
        .map(|(k, v)| (v.as_str(), k.as_str()))
        .collect();
    for (key, _) in pairs.iter_mut() {
        if let Some(source_name) = reverse.get(key.as_str()) {
            *key = source_name.to_string();
        }
    }
}

/// Rewrite every URL found in a `Link` header so it points at `proxy_base`
/// instead of `target_base`, preserving query strings (with pagination
/// param names translated back to source-side names).
pub fn rewrite_link_header(
    value: &str,
    target_base: &Url,
    proxy_base: &Url,
    settings: &PaginationSettings,
) -> String {
    // Link header format: <url>; rel="next", <url>; rel="prev", ...
    value
        .split(',')
        .map(|part| rewrite_link_segment(part, target_base, proxy_base, settings))
        .collect::<Vec<_>>()
        .join(",")
}

fn rewrite_link_segment(
    segment: &str,
    target_base: &Url,
    proxy_base: &Url,
    settings: &PaginationSettings,
) -> String {
    let Some(start) = segment.find('<') else {
        return segment.to_string();
    };
    let Some(end) = segment[start..].find('>') else {
        return segment.to_string();
    };
    let end = start + end;
    let url_str = &segment[start + 1..end];

    let Ok(mut parsed) = Url::parse(url_str) else {
        return segment.to_string();
    };

    if parsed.scheme() != target_base.scheme() || parsed.host_str() != target_base.host_str() {
        return segment.to_string();
    }

    let mut pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    translate_query_params_reverse(&mut pairs, settings);

    let _ = parsed.set_scheme(proxy_base.scheme());
    let _ = parsed.set_host(proxy_base.host_str());
    let _ = parsed.set_port(proxy_base.port());
    {
        let mut qp = parsed.query_pairs_mut();
        qp.clear();
        for (k, v) in &pairs {
            qp.append_pair(k, v);
        }
    }

    format!("{}<{}>{}", &segment[..start], parsed.as_str(), &segment[end + 1..])
}

/// Apply header renames from the endpoint mapping to a response header map.
pub fn apply_header_renames(headers: &mut HeaderMap, renames: &std::collections::BTreeMap<String, String>) {
    for (source_name, target_name) in renames {
        if let Some(val) = headers.remove(target_name.as_str()) {
            if let Ok(name) = http::HeaderName::from_bytes(source_name.as_bytes()) {
                headers.insert(name, val);
            }
        }
    }
}
