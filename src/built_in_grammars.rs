//! Query-store compatibility records. Official metadata lives in builtin_langs.
use crate::builtin_langs::{LanguageDescriptor, OFFICIAL_LANGUAGES};
use tree_sitter_tags::TagsConfiguration;

pub struct BuiltinGrammar {
    pub extensions: &'static [&'static str],
    pub config: QueryConfig,
}

pub enum QueryConfig {
    /// Compiled lazily through the shared store, keyed by language identity.
    Official(&'static LanguageDescriptor),
    /// User-provided native grammars retain their existing extension overrides.
    User(Result<TagsConfiguration, tree_sitter_tags::Error>),
}

pub fn load() -> Vec<BuiltinGrammar> {
    OFFICIAL_LANGUAGES
        .iter()
        .filter(|desc| desc.query.is_some())
        .map(|desc| BuiltinGrammar {
            extensions: desc.extensions,
            config: QueryConfig::Official(desc),
        })
        .collect()
}
