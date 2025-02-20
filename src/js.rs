use tree_sitter_javascript;
use tree_sitter_tags::TagsConfiguration;

pub fn get_tags_config() -> TagsConfiguration {
    TagsConfiguration::new(
        tree_sitter_javascript::LANGUAGE.into(),
        tree_sitter_javascript::TAGS_QUERY,
        "",
    )
    .unwrap()
}
