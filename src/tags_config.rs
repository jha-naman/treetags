use tree_sitter::Language;
use tree_sitter_tags::TagsConfiguration;

pub fn get_tags_config(language: Language, tags_query: &str) -> TagsConfiguration {
    TagsConfiguration::new(language, tags_query, "").unwrap()
}
