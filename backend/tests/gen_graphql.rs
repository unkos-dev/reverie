use std::path::PathBuf;

#[test]
fn graphql_schema_matches_committed_artifact() {
    let rendered = reverie_api::graphql::schema(reverie_api::graphql::ExecutionPolicy::Development)
        .sdl_with_options(async_graphql::SDLExportOptions::new().use_space_ident());
    let path =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo manifest directory"))
            .join("schema.graphql");
    if std::env::var_os("REGEN").is_some() {
        std::fs::write(path, rendered).expect("write GraphQL schema");
        return;
    }
    assert_eq!(
        std::fs::read_to_string(path).expect("read GraphQL schema"),
        rendered,
        "GraphQL schema is stale; run just rust::regen"
    );
}
