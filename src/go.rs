use tree_sitter_go;
use tree_sitter_tags::TagsConfiguration;

pub fn get_tags_config() -> TagsConfiguration {
    TagsConfiguration::new(
        tree_sitter_go::LANGUAGE.into(),
        tree_sitter_go::TAGS_QUERY,
        "",
    )
    .unwrap()
}
