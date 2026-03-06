use tree_sitter::Language;
use tree_sitter_tags::TagsConfiguration;

pub fn get_tags_config(
    language: Language,
    tags_query: &str,
    lang_name: &str,
) -> Result<TagsConfiguration, tree_sitter_tags::Error> {
    match TagsConfiguration::new(language, tags_query, "") {
        Ok(config) => Ok(config),
        Err(e) => {
            eprintln!(
                "Error creating tags configuration for {} language: {}",
                lang_name, e
            );
            Err(e)
        }
    }
}
