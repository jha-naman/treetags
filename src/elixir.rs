use tree_sitter_elixir;
use tree_sitter_tags::TagsConfiguration;

pub fn get_tags_config() -> TagsConfiguration {
    TagsConfiguration::new(
        tree_sitter_elixir::LANGUAGE.into(),
        tree_sitter_elixir::TAGS_QUERY,
        "",
    )
    .unwrap()
}
