use tree_sitter_ocaml;
use tree_sitter_tags::TagsConfiguration;

pub fn get_tags_config() -> TagsConfiguration {
    TagsConfiguration::new(
        tree_sitter_ocaml::LANGUAGE_OCAML.into(),
        tree_sitter_ocaml::TAGS_QUERY,
        "",
    )
    .unwrap()
}
