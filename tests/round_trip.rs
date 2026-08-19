use axum::body::{to_bytes, Body};
use axum::extract::{Request, State};
use serde_json::json;
use shotgun::mapping::types::{EndpointMapping, FieldMapping, MappingFile, Meta, Settings};
use shotgun::proxy::handler::{proxy_handler, AppState};
use std::collections::BTreeMap;
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn mapping_for(target_base: &str) -> MappingFile {
    let mut path_params = BTreeMap::new();
    path_params.insert("petId".to_string(), "id".to_string());

    MappingFile {
        meta: Meta::default(),
        settings: Settings {
            target_base_path: target_base.to_string(),
            ..Default::default()
        },
        endpoints: vec![EndpointMapping {
            source: "GET /pets/{petId}".to_string(),
            target: "GET /pets/{id}".to_string(),
            path_params,
            response: FieldMapping {
                renames: [("created_at".to_string(), "created".to_string())].into_iter().collect(),
                ..Default::default()
            },
            ..Default::default()
        }],
        schemas: vec![],
    }
}

#[tokio::test]
async fn round_trips_a_request_through_the_proxy() {
    let upstream = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/pets/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 42,
            "created": "2026-01-01"
        })))
        .mount(&upstream)
        .await;

    let mapping = mapping_for("/api/v3");
    let state = Arc::new(AppState {
        mapping,
        target_url: url::Url::parse(&upstream.uri()).unwrap(),
        client: reqwest::Client::new(),
        log_unmapped: false,
    });

    let req = Request::builder()
        .method("GET")
        .uri("/pets/42")
        .body(Body::empty())
        .unwrap();

    let resp = proxy_handler(State(state), req).await;
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(status, 200);
    assert_eq!(body["id"], json!(42));
    assert_eq!(body["created_at"], json!("2026-01-01"));
    assert!(body.get("created").is_none());
}

#[tokio::test]
async fn unmapped_endpoint_returns_501_by_default() {
    let upstream = MockServer::start().await;
    let mapping = mapping_for("/api/v3");
    let state = Arc::new(AppState {
        mapping,
        target_url: url::Url::parse(&upstream.uri()).unwrap(),
        client: reqwest::Client::new(),
        log_unmapped: false,
    });

    let req = Request::builder()
        .method("GET")
        .uri("/does-not-exist")
        .body(Body::empty())
        .unwrap();

    let resp = proxy_handler(State(state), req).await;
    assert_eq!(resp.status(), 501);
}
