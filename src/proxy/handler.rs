use crate::mapping::types::{EndpointMapping, MappingFile, UnmappedEndpointBehavior};
use crate::proxy::pagination::{rewrite_link_header, translate_query_params};
use crate::proxy::transform::{transform_request, transform_response, SchemaRegistry};
use axum::body::{to_bytes, Body};
use axum::extract::State;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::sync::Arc;
use url::Url;

pub struct AppState {
    pub mapping: MappingFile,
    pub target_url: Url,
    pub client: reqwest::Client,
    pub log_unmapped: bool,
}

const MAX_BODY_BYTES: usize = 100 * 1024 * 1024;

/// Match an incoming (method, path) against the mapping file's source
/// endpoints. Path params are matched positionally by `{name}` segments.
pub fn match_endpoint<'a>(
    mapping: &'a MappingFile,
    method: &Method,
    path: &str,
) -> Option<(&'a EndpointMapping, HashMap<String, String>)> {
    let req_segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    for ep in &mapping.endpoints {
        let Some((ep_method, ep_path)) = EndpointMapping::method_and_path(&ep.source) else {
            continue;
        };
        if !ep_method.eq_ignore_ascii_case(method.as_str()) {
            continue;
        }
        let pat_segs: Vec<&str> = ep_path.split('/').filter(|s| !s.is_empty()).collect();
        if pat_segs.len() != req_segs.len() {
            continue;
        }
        let mut params = HashMap::new();
        let mut ok = true;
        for (pat, val) in pat_segs.iter().zip(req_segs.iter()) {
            if pat.starts_with('{') && pat.ends_with('}') {
                params.insert(pat[1..pat.len() - 1].to_string(), val.to_string());
            } else if pat != val {
                ok = false;
                break;
            }
        }
        if ok {
            return Some((ep, params));
        }
    }
    None
}

/// Build the upstream target URL for a matched endpoint, substituting path
/// params (translated via `endpoint.path_params` when the target uses a
/// different param name) and applying `target_base_path`.
fn build_target_path(
    mapping: &MappingFile,
    endpoint: &EndpointMapping,
    source_params: &HashMap<String, String>,
) -> String {
    let (_, target_pattern) = EndpointMapping::method_and_path(&endpoint.target).unwrap_or_default();
    let mut path = String::new();
    for seg in target_pattern.split('/') {
        if seg.is_empty() {
            continue;
        }
        path.push('/');
        if seg.starts_with('{') && seg.ends_with('}') {
            let target_param_name = &seg[1..seg.len() - 1];
            // Find the source param name whose renamed target equals this,
            // falling back to same-name lookup.
            let source_name = endpoint
                .path_params
                .iter()
                .find(|(_, v)| v.as_str() == target_param_name)
                .map(|(k, _)| k.as_str())
                .unwrap_or(target_param_name);
            if let Some(v) = source_params.get(source_name) {
                path.push_str(v);
            }
        } else {
            path.push_str(seg);
        }
    }
    format!("{}{}", mapping.settings.target_base_path, path)
}

pub async fn proxy_handler(State(state): State<Arc<AppState>>, req: axum::extract::Request) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req.uri().query().unwrap_or("").to_string();

    let Some((endpoint, params)) = match_endpoint(&state.mapping, &method, &path) else {
        if state.log_unmapped {
            tracing::warn!(%method, %path, "request to unmapped endpoint");
        }
        return match state.mapping.settings.unmapped_endpoint_behavior {
            UnmappedEndpointBehavior::Reject => {
                (StatusCode::NOT_IMPLEMENTED, "no mapping for this endpoint").into_response()
            }
            UnmappedEndpointBehavior::Passthrough => {
                forward_passthrough(&state, &method, &path, &query, req).await
            }
        };
    };

    if endpoint.target.is_empty() {
        return (
            StatusCode::NOT_IMPLEMENTED,
            format!("endpoint '{}' has no target configured in mappings.toml", endpoint.source),
        )
            .into_response();
    }

    let target_path = build_target_path(&state.mapping, endpoint, &params);
    let mut target_url = state.target_url.clone();
    target_url.set_path(&target_path);

    let mut pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    for (k, _) in pairs.iter_mut() {
        if let Some(renamed) = endpoint.query_params.get(k.as_str()) {
            *k = renamed.clone();
        }
    }
    translate_query_params(&mut pairs, &state.mapping.settings.pagination);
    {
        let mut qp = target_url.query_pairs_mut();
        qp.clear();
        for (k, v) in &pairs {
            qp.append_pair(k, v);
        }
    }

    let (parts, body) = req.into_parts();
    let body_bytes = match to_bytes(body, MAX_BODY_BYTES).await {
        Ok(b) => b,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("failed to read body: {e}")).into_response()
        }
    };

    let out_body = if !body_bytes.is_empty() {
        match serde_json::from_slice::<serde_json::Value>(&body_bytes) {
            Ok(mut json) => {
                transform_request(&mut json, endpoint);
                serde_json::to_vec(&json).unwrap_or_else(|_| body_bytes.to_vec())
            }
            Err(_) => body_bytes.to_vec(),
        }
    } else {
        Vec::new()
    };

    let mut upstream_req = state
        .client
        .request(
            reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::GET),
            target_url.clone(),
        )
        .body(out_body);

    for (name, value) in parts.headers.iter() {
        let name_str = name.as_str();
        if name_str.eq_ignore_ascii_case("host") || name_str.eq_ignore_ascii_case("content-length") {
            continue;
        }
        let header_name = endpoint
            .headers
            .get(name_str)
            .map(|s| s.as_str())
            .unwrap_or(name_str);
        if let (Ok(n), Ok(v)) = (
            reqwest::header::HeaderName::from_bytes(header_name.as_bytes()),
            reqwest::header::HeaderValue::from_bytes(value.as_bytes()),
        ) {
            upstream_req = upstream_req.header(n, v);
        }
    }

    let upstream_resp = match upstream_req.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, %target_url, "upstream request failed");
            return (StatusCode::BAD_GATEWAY, format!("upstream error: {e}")).into_response();
        }
    };

    let status = upstream_resp.status();
    let mut resp_headers = HeaderMap::new();
    for (name, value) in upstream_resp.headers().iter() {
        // The body below is a fully-materialized byte buffer, not a
        // passthrough stream -- forwarding the upstream's own framing
        // headers (chunked transfer-encoding, content-encoding for a body
        // we may have already re-serialized) alongside hyper's own
        // Content-Length produces an invalid response that most clients
        // silently drop the connection on.
        if name.as_str().eq_ignore_ascii_case("transfer-encoding")
            || name.as_str().eq_ignore_ascii_case("content-encoding")
        {
            continue;
        }
        if let (Ok(n), Ok(v)) = (
            HeaderName::from_bytes(name.as_str().as_bytes()),
            HeaderValue::from_bytes(value.as_bytes()),
        ) {
            if name.as_str().eq_ignore_ascii_case("link") && state.mapping.settings.pagination.rewrite_link_urls {
                if let Ok(vs) = v.to_str() {
                    let proxy_base = Url::parse("http://proxy.local").unwrap();
                    let rewritten =
                        rewrite_link_header(vs, &state.target_url, &proxy_base, &state.mapping.settings.pagination);
                    if let Ok(hv) = HeaderValue::from_str(&rewritten) {
                        resp_headers.insert(n, hv);
                        continue;
                    }
                }
            }
            resp_headers.insert(n, v);
        }
    }
    crate::proxy::pagination::apply_header_renames(&mut resp_headers, &endpoint.headers);
    for (name, value) in &state.mapping.settings.synthesized_response_headers {
        if let (Ok(n), Ok(v)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            resp_headers.insert(n, v);
        }
    }

    let resp_bytes = match upstream_resp.bytes().await {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("failed reading upstream body: {e}")).into_response(),
    };

    let out_bytes = match serde_json::from_slice::<serde_json::Value>(&resp_bytes) {
        Ok(mut json) => {
            let registry = SchemaRegistry::from_mapping_file(&state.mapping);
            transform_response(&mut json, endpoint, &registry, &state.mapping.settings.unmapped_field_behavior);
            serde_json::to_vec(&json).unwrap_or_else(|_| resp_bytes.to_vec())
        }
        Err(_) => resp_bytes.to_vec(),
    };

    let mut builder = Response::builder().status(status.as_u16());
    for (name, value) in resp_headers.iter() {
        if name.as_str().eq_ignore_ascii_case("content-length") {
            continue;
        }
        builder = builder.header(name, value);
    }
    builder.body(Body::from(out_bytes)).unwrap_or_else(|_| {
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    })
}

async fn forward_passthrough(
    state: &Arc<AppState>,
    method: &Method,
    path: &str,
    query: &str,
    req: axum::extract::Request,
) -> Response {
    let mut target_url = state.target_url.clone();
    target_url.set_path(path);
    target_url.set_query(if query.is_empty() { None } else { Some(query) });

    let (parts, body) = req.into_parts();
    let body_bytes = to_bytes(body, MAX_BODY_BYTES).await.unwrap_or_default();

    let mut upstream_req = state.client.request(
        reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::GET),
        target_url,
    );
    for (name, value) in parts.headers.iter() {
        if name.as_str().eq_ignore_ascii_case("host") {
            continue;
        }
        if let (Ok(n), Ok(v)) = (
            reqwest::header::HeaderName::from_bytes(name.as_str().as_bytes()),
            reqwest::header::HeaderValue::from_bytes(value.as_bytes()),
        ) {
            upstream_req = upstream_req.header(n, v);
        }
    }
    upstream_req = upstream_req.body(body_bytes.to_vec());

    match upstream_req.send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let bytes = resp.bytes().await.unwrap_or_default();
            (StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY), bytes.to_vec()).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("upstream error: {e}")).into_response(),
    }
}
