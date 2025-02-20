use tree_sitter_php;
use tree_sitter_tags::TagsConfiguration;

pub fn get_tags_config() -> TagsConfiguration {
    TagsConfiguration::new(
        tree_sitter_php::LANGUAGE_PHP.into(),
        tree_sitter_php::TAGS_QUERY,
        "",
    )
    .unwrap()
}
