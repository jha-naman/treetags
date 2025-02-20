use tree_sitter_rust;
use tree_sitter_tags::TagsConfiguration;

pub fn get_tags_config() -> TagsConfiguration {
    TagsConfiguration::new(
        tree_sitter_rust::LANGUAGE.into(),
        tree_sitter_rust::TAGS_QUERY,
        "",
    )
    .unwrap()
}
