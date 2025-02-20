use tree_sitter_ruby;
use tree_sitter_tags::TagsConfiguration;

pub fn get_tags_config() -> TagsConfiguration {
    TagsConfiguration::new(
        tree_sitter_ruby::LANGUAGE.into(),
        tree_sitter_ruby::TAGS_QUERY,
        "",
    )
    .unwrap()
}
