use tree_sitter_c;
use tree_sitter_tags::TagsConfiguration;

pub fn get_tags_config() -> TagsConfiguration {
    TagsConfiguration::new(
        tree_sitter_c::LANGUAGE.into(),
        tree_sitter_c::TAGS_QUERY,
        "",
    )
    .unwrap()
}
