use crate::queries;
use crate::tags_config::get_tags_config;
use tree_sitter_tags::TagsConfiguration;

/// A built-in query-based grammar: its canonical language name, `--language-force`
/// aliases, file extensions, and compiled tags configuration.
pub struct BuiltinGrammar {
    pub lang: &'static str,
    pub aliases: &'static [&'static str],
    pub extensions: &'static [&'static str],
    /// `fnmatch`-style filename globs (matched against the basename) that select
    /// this language, e.g. `Rakefile` or `*.gemspec`.
    pub patterns: &'static [&'static str],
    /// Interpreter names matched against a `#!` shebang line, e.g. `ruby`.
    pub interpreters: &'static [&'static str],
    pub config: Result<TagsConfiguration, tree_sitter_tags::Error>,
}

pub fn load() -> Vec<BuiltinGrammar> {
    vec![
        BuiltinGrammar {
            lang: "ruby",
            aliases: &[],
            extensions: &["rb"],
            patterns: &["Rakefile", "Gemfile", "Guardfile", "Vagrantfile", "Podfile", "Berksfile", "Brewfile", "Capfile", "*.gemspec", "*.podspec", "*.rake"],
            interpreters: &["ruby"],
            config: get_tags_config(
                tree_sitter_ruby::LANGUAGE.into(),
                tree_sitter_ruby::TAGS_QUERY,
                "ruby",
            ),
        },
        BuiltinGrammar {
            lang: "python",
            aliases: &[],
            extensions: &["py", "pyw"],
            patterns: &[],
            interpreters: &[],
            config: get_tags_config(
                tree_sitter_python::LANGUAGE.into(),
                tree_sitter_python::TAGS_QUERY,
                "python",
            ),
        },
        BuiltinGrammar {
            lang: "c",
            aliases: &[],
            extensions: &["c", "h", "i"],
            patterns: &[],
            interpreters: &[],
            config: get_tags_config(
                tree_sitter_c::LANGUAGE.into(),
                tree_sitter_c::TAGS_QUERY,
                "c",
            ),
        },
        BuiltinGrammar {
            lang: "c++",
            aliases: &["cpp", "cxx", "cplusplus"],
            extensions: &[
                "cc", "cpp", "CPP", "cxx", "c++", "cp", "C", "cppm", "ixx", "ii", "H", "hh", "hpp",
                "HPP", "hxx", "h++", "tcc",
            ],
            patterns: &[],
            interpreters: &[],
            config: get_tags_config(
                tree_sitter_cpp::LANGUAGE.into(),
                tree_sitter_cpp::TAGS_QUERY,
                "c++",
            ),
        },
        BuiltinGrammar {
            lang: "java",
            aliases: &[],
            extensions: &["java"],
            patterns: &[],
            interpreters: &[],
            config: get_tags_config(
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
            config: get_tags_config(
                tree_sitter_ocaml::LANGUAGE_OCAML.into(),
                tree_sitter_ocaml::TAGS_QUERY,
                "ocaml",
            ),
        },
        BuiltinGrammar {
            lang: "php",
            aliases: &[],
            extensions: &["php"],
            patterns: &[],
            interpreters: &["php"],
            config: get_tags_config(
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
            config: get_tags_config(
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
            config: get_tags_config(
                tree_sitter_lua::LANGUAGE.into(),
                tree_sitter_lua::TAGS_QUERY,
                "lua",
            ),
        },
        BuiltinGrammar {
            lang: "c#",
            aliases: &["csharp"],
            extensions: &["cs"],
            patterns: &[],
            interpreters: &[],
            config: get_tags_config(
                tree_sitter_c_sharp::LANGUAGE.into(),
                queries::C_SHARP_TAGS_QUERY,
                "c#",
            ),
        },
        BuiltinGrammar {
            lang: "shell",
            aliases: &["sh", "bash"],
            extensions: &["sh", "bash"],
            patterns: &[".bashrc", ".bash_profile", ".bash_logout", ".zshrc", ".zprofile", ".zshenv", ".profile", "PKGBUILD", "*.zsh"],
            interpreters: &["sh", "bash", "dash", "zsh", "ksh"],
            config: get_tags_config(
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
            config: get_tags_config(
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
            config: get_tags_config(
                tree_sitter_julia::LANGUAGE.into(),
                queries::JULIA_TAGS_QUERY,
                "julia",
            ),
        },
    ]
}
