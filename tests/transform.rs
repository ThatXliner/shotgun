use serde_json::json;
use shotgun::mapping::types::{EndpointMapping, FieldMapping, MappingFile, NestedMapping, SchemaMapping, UnmappedFieldBehavior};
use shotgun::proxy::transform::{transform_request, transform_response, SchemaRegistry};

fn sample_endpoint() -> EndpointMapping {
    EndpointMapping {
        source: "GET /pets/{id}".to_string(),
        target: "GET /pets/{id}".to_string(),
        response: FieldMapping {
            renames: [("created_at".to_string(), "created".to_string())].into_iter().collect(),
            defaults: [("node_id".to_string(), json!(""))].into_iter().collect(),
            drops: vec!["internal_note".to_string()],
            type_conflicts: Default::default(),
            nested: vec![NestedMapping {
                path: "owner".to_string(),
                schema_map: "Owner".to_string(),
            }],
        },
        ..Default::default()
    }
}

fn sample_mapping_file() -> MappingFile {
    let mut mf = MappingFile::default();
    mf.endpoints.push(sample_endpoint());
    mf.schemas.push(SchemaMapping {
        name: "Owner".to_string(),
        edited: false,
        renames: [("name".to_string(), "full_name".to_string())].into_iter().collect(),
        defaults: Default::default(),
        drops: vec![],
        type_conflicts: Default::default(),
    });
    mf
}

#[test]
fn transform_response_applies_renames_defaults_and_drops() {
    let mf = sample_mapping_file();
    let endpoint = &mf.endpoints[0];
    let registry = SchemaRegistry::from_mapping_file(&mf);

    let mut body = json!({
        "id": 1,
        "created": "2026-01-01",
        "internal_note": "secret",
        "owner": { "id": 5, "full_name": "Alice" }
    });

    transform_response(&mut body, endpoint, &registry, &UnmappedFieldBehavior::Passthrough);

    assert_eq!(body["created_at"], json!("2026-01-01"));
    assert!(body.get("created").is_none());
    assert!(body.get("internal_note").is_none());
    assert_eq!(body["node_id"], json!(""));
    assert_eq!(body["owner"]["name"], json!("Alice"));
    assert!(body["owner"].get("full_name").is_none());
}

#[test]
fn transform_response_handles_arrays() {
    let mf = sample_mapping_file();
    let endpoint = &mf.endpoints[0];
    let registry = SchemaRegistry::from_mapping_file(&mf);

    let mut body = json!([
        { "id": 1, "created": "a", "owner": { "full_name": "Alice" } },
        { "id": 2, "created": "b", "owner": { "full_name": "Bob" } }
    ]);

    transform_response(&mut body, endpoint, &registry, &UnmappedFieldBehavior::Passthrough);

    assert_eq!(body[0]["created_at"], json!("a"));
    assert_eq!(body[1]["owner"]["name"], json!("Bob"));
}

#[test]
fn transform_request_applies_renames_in_reverse() {
    let mut endpoint = sample_endpoint();
    endpoint.request = endpoint.response.clone();
    let mut body = json!({ "created_at": "2026-01-01", "node_id": "should be stripped" });

    transform_request(&mut body, &endpoint);

    // source -> target direction: created_at becomes created
    assert_eq!(body["created"], json!("2026-01-01"));
    assert!(body.get("created_at").is_none());
    // node_id is a source-only default field; target doesn't understand it.
    assert!(body.get("node_id").is_none());
}
