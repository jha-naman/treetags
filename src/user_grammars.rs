use crate::config::Config;
use crate::queries;
use crate::tags_config::get_tags_config;
use libloading::{Library, Symbol};
use std::fs;
use tree_sitter::{Language, LANGUAGE_VERSION, MIN_COMPATIBLE_LANGUAGE_VERSION};
use tree_sitter_tags::TagsConfiguration;

pub struct UserGrammars {
    pub tag_configurations: Vec<(
        Vec<String>,
        Result<TagsConfiguration, tree_sitter_tags::Error>,
    )>,
    pub _grammars: Vec<Library>,
}

struct LanguageDefaults {
    query: &'static str,
    extensions: &'static [&'static str],
}

fn get_language_defaults(lang_name: &str) -> Option<LanguageDefaults> {
    match lang_name {
        "kotlin" => Some(LanguageDefaults {
            query: queries::KOTLIN_TAGS_QUERY,
            extensions: &["kt", "kts"],
        }),
        _ => None,
    }
}

pub fn load(config: &Config) -> UserGrammars {
    let mut tag_configurations = Vec::new();
    let mut grammars = Vec::new();

    for user_grammar in &config.user_grammars {
        unsafe {
            let lib = match Library::new(&user_grammar.grammar_lib_path) {
                Ok(lib) => lib,
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to load grammar from {}: {}",
                        user_grammar.grammar_lib_path.display(),
                        e
                    );
                    continue;
                }
            };

            let language_func_name = format!("tree_sitter_{}", user_grammar.language_name);
            let language_symbol: Result<Symbol<unsafe extern "C" fn() -> Language>, _> =
                lib.get(language_func_name.as_bytes());

            let language = match language_symbol {
                Ok(lang_fn) => {
                    let language_fn = lang_fn();
                    let lang_version = language_fn.version();
                    if lang_version > LANGUAGE_VERSION
                        || lang_version < MIN_COMPATIBLE_LANGUAGE_VERSION
                    {
                        eprintln!(
                            "Warning: Grammar '{}' has incompatible version {} (expected {})",
                            user_grammar.language_name,
                            language_fn.version(),
                            tree_sitter::LANGUAGE_VERSION
                        );
                        continue;
                    }
                    language_fn
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to find language function '{}' in {}: {}",
                        language_func_name,
                        user_grammar.grammar_lib_path.display(),
                        e
                    );
                    continue;
                }
            };

            let user_query = if let Some(query_path) = &user_grammar.query_file_path {
                match fs::read_to_string(query_path) {
                    Ok(query) => Some(query),
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to read query file {}: {}",
                            query_path.display(),
                            e
                        );
                        None
                    }
                }
            } else {
                None
            };

            let defaults = get_language_defaults(&user_grammar.language_name);

            let tags_query = if let Some(query) = user_query {
                query
            } else if let Some(defaults) = &defaults {
                defaults.query.to_string()
            } else {
                "".to_string()
            };

            let extensions = if let Some(exts) = &user_grammar.extensions {
                exts.clone()
            } else if let Some(defaults) = &defaults {
                defaults.extensions.iter().map(|s| s.to_string()).collect()
            } else {
                vec![]
            };

            let tags_config = get_tags_config(language, &tags_query, &user_grammar.language_name);
            tag_configurations.push((extensions, tags_config));
            grammars.push(lib);
        }
    }

    UserGrammars {
        tag_configurations,
        _grammars: grammars,
    }
}
