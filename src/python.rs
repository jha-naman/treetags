use tree_sitter_python;
use tree_sitter_tags::TagsConfiguration;

pub fn get_tags_config() -> TagsConfiguration {
    TagsConfiguration::new(
        tree_sitter_python::LANGUAGE.into(),
        tree_sitter_python::TAGS_QUERY,
        "",
    )
    .unwrap()
}
