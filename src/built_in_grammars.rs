use crate::queries;
use crate::tags_config::get_tags_config;
use tree_sitter_tags::TagsConfiguration;

/// A built-in query-based grammar: its canonical language name, `--language-force`
/// aliases, file extensions, and bundled or external tags configuration.
pub struct BuiltinGrammar {
    pub lang: &'static str,
    pub aliases: &'static [&'static str],
    pub extensions: &'static [&'static str],
    /// `fnmatch`-style filename globs (matched against the basename) that select
    /// this language, e.g. `Rakefile` or `*.gemspec`.
    pub patterns: &'static [&'static str],
    /// Interpreter names matched against a `#!` shebang line, e.g. `ruby`.
    pub interpreters: &'static [&'static str],
    pub config: QueryConfig,
}

pub enum QueryConfig {
    Bundled(Result<TagsConfiguration, tree_sitter_tags::Error>),
    Wasm(&'static crate::wasm_grammars::WasmGrammar),
}
impl QueryConfig {
    pub fn is_err(&self) -> bool {
        matches!(self, Self::Bundled(Err(_)))
    }
}
fn get_query_config(language: tree_sitter::Language, query: &str, name: &str) -> QueryConfig {
    QueryConfig::Bundled(get_tags_config(language, query, name))
}

pub fn load() -> Vec<BuiltinGrammar> {
    vec![
        BuiltinGrammar {
            lang: "ruby",
            aliases: &[],
            extensions: &["rb"],
            patterns: &[
                "Rakefile",
                "Gemfile",
                "Guardfile",
                "Vagrantfile",
                "Podfile",
                "Berksfile",
                "Brewfile",
                "Capfile",
                "*.gemspec",
                "*.podspec",
                "*.rake",
            ],
            interpreters: &["ruby"],
            config: get_query_config(
                tree_sitter_ruby::LANGUAGE.into(),
                tree_sitter_ruby::TAGS_QUERY,
                "ruby",
            ),
        },
        BuiltinGrammar {
            lang: "java",
            aliases: &[],
            extensions: &["java"],
            patterns: &[],
            interpreters: &[],
            config: get_query_config(
                tree_sitter_java::LANGUAGE.into(),
                tree_sitter_java::TAGS_QUERY,
                "java",
            ),
        },
        BuiltinGrammar {
            lang: "ocaml",
            aliases: &[],
            extensions: &["ml"],
            patterns: &[],
            interpreters: &[],
            config: QueryConfig::Wasm(&crate::wasm_grammars::OCAML),
        },
        BuiltinGrammar {
            lang: "php",
            aliases: &[],
            extensions: &["php"],
            patterns: &[],
            interpreters: &["php"],
            config: get_query_config(
                tree_sitter_php::LANGUAGE_PHP.into(),
                tree_sitter_php::TAGS_QUERY,
                "php",
            ),
        },
        BuiltinGrammar {
            lang: "elixir",
            aliases: &[],
            extensions: &["ex"],
            patterns: &[],
            interpreters: &[],
            config: get_query_config(
                tree_sitter_elixir::LANGUAGE.into(),
                tree_sitter_elixir::TAGS_QUERY,
                "elixir",
            ),
        },
        BuiltinGrammar {
            lang: "lua",
            aliases: &[],
            extensions: &["lua"],
            patterns: &[],
            interpreters: &["lua"],
            config: get_query_config(
                tree_sitter_lua::LANGUAGE.into(),
                tree_sitter_lua::TAGS_QUERY,
                "lua",
            ),
        },
        BuiltinGrammar {
            lang: "shell",
            aliases: &["sh", "bash"],
            extensions: &["sh", "bash"],
            patterns: &[
                ".bashrc",
                ".bash_profile",
                ".bash_logout",
                ".zshrc",
                ".zprofile",
                ".zshenv",
                ".profile",
                "PKGBUILD",
                "*.zsh",
            ],
            interpreters: &["sh", "bash", "dash", "zsh", "ksh"],
            config: get_query_config(
                tree_sitter_bash::LANGUAGE.into(),
                queries::BASH_TAGS_QUERY,
                "shell",
            ),
        },
        BuiltinGrammar {
            lang: "scala",
            aliases: &[],
            extensions: &["scala"],
            patterns: &[],
            interpreters: &["scala"],
            config: get_query_config(
                tree_sitter_scala::LANGUAGE.into(),
                queries::SCALA_TAGS_QUERY,
                "scala",
            ),
        },
        BuiltinGrammar {
            lang: "julia",
            aliases: &[],
            extensions: &["jl"],
            patterns: &[],
            interpreters: &["julia"],
            config: get_query_config(
                tree_sitter_julia::LANGUAGE.into(),
                queries::JULIA_TAGS_QUERY,
                "julia",
            ),
        },
    ]
}
