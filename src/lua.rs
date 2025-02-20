use tree_sitter_lua;
use tree_sitter_tags::TagsConfiguration;

pub fn get_tags_config() -> TagsConfiguration {
    TagsConfiguration::new(
        tree_sitter_lua::LANGUAGE.into(),
        tree_sitter_lua::TAGS_QUERY,
        "",
    )
    .unwrap()
}
