use tree_sitter_cpp;
use tree_sitter_tags::TagsConfiguration;

pub fn get_tags_config() -> TagsConfiguration {
    TagsConfiguration::new(
        tree_sitter_cpp::LANGUAGE.into(),
        tree_sitter_cpp::TAGS_QUERY,
        "",
    )
    .unwrap()
}
