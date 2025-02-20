use tree_sitter_java;
use tree_sitter_tags::TagsConfiguration;

pub fn get_tags_config() -> TagsConfiguration {
    TagsConfiguration::new(
        tree_sitter_java::LANGUAGE.into(),
        tree_sitter_java::TAGS_QUERY,
        "",
    )
    .unwrap()
}
