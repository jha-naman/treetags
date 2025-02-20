use tree_sitter_tags::TagsConfiguration;
use tree_sitter_typescript;

pub fn get_tags_config() -> TagsConfiguration {
    TagsConfiguration::new(
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        tree_sitter_typescript::TAGS_QUERY,
        "",
    )
    .unwrap()
}
