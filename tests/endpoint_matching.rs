use shotgun::diff::build_mapping_file;
use shotgun::spec::parser::parse_spec_file;

fn load() -> (shotgun::spec::model::ApiSpec, shotgun::spec::model::ApiSpec) {
    let source = parse_spec_file("tests/fixtures/petstore_v2.json").unwrap();
    let target = parse_spec_file("tests/fixtures/petstore_v3.json").unwrap();
    (source, target)
}

#[test]
fn parses_both_spec_versions() {
    let (source, target) = load();
    assert_eq!(source.endpoints.len(), 3);
    assert_eq!(target.endpoints.len(), 3);
    assert!(source.schemas.contains_key("Pet"));
    assert!(target.schemas.contains_key("Pet"));
}

#[test]
fn maps_every_endpoint_deterministically_by_path() {
    let (source, target) = load();
    let (mapping, summary) =
        build_mapping_file(&source, &target, "petstore_v2.json", "petstore_v3.json", "2026-08-18T00:00:00Z");

    // All three endpoints share the same normalized path + method in both
    // specs (only the path param *name* differs), so all three should
    // auto-map with no unmatched/method-mismatch fallout.
    assert_eq!(summary.total_source_endpoints, 3);
    assert_eq!(summary.mapped, 3, "all three endpoints should auto-map: {mapping:#?}");
    assert_eq!(summary.unmatched, 0);
    assert_eq!(summary.method_mismatches, 0);
    assert!(mapping.endpoints.iter().all(|e| !e.target.is_empty()));
}

#[test]
fn detects_path_param_rename() {
    let (source, target) = load();
    let (mapping, _) =
        build_mapping_file(&source, &target, "petstore_v2.json", "petstore_v3.json", "2026-08-18T00:00:00Z");

    let get_pet = mapping
        .endpoints
        .iter()
        .find(|e| e.source == "GET /pets/{petId}")
        .expect("GET /pets/{petId} should be present");

    assert_eq!(get_pet.target, "GET /pets/{id}");
    assert_eq!(get_pet.path_params.get("petId"), Some(&"id".to_string()));
}

#[test]
fn does_not_guess_renames_for_differently_named_fields() {
    let (source, target) = load();
    let (mapping, _) =
        build_mapping_file(&source, &target, "petstore_v2.json", "petstore_v3.json", "2026-08-18T00:00:00Z");

    let get_pet = mapping
        .endpoints
        .iter()
        .find(|e| e.source == "GET /pets/{petId}")
        .unwrap();

    // No rename inference: "created_at" (source-only) becomes a default,
    // and "created"/"internal_note" (target-only) become drops -- Shotgun
    // never guesses that they're the same field.
    assert!(get_pet.response.renames.is_empty());
    assert!(get_pet.response.defaults.contains_key("created_at"));
    assert!(get_pet.response.drops.contains(&"created".to_string()));
    assert!(get_pet.response.drops.contains(&"internal_note".to_string()));

    // Owner is still a nested schema and gets registered for reuse, but
    // again with no guessed rename between "name" and "full_name".
    let owner_schema = mapping.schemas.iter().find(|s| s.name == "Owner").unwrap();
    assert!(owner_schema.renames.is_empty());
    assert!(owner_schema.defaults.contains_key("name"));
    assert!(owner_schema.drops.contains(&"full_name".to_string()));
}
