use crate::queries;
use crate::tags_config::get_tags_config;
use tree_sitter_tags::TagsConfiguration;

pub fn load() -> Vec<(
    Vec<&'static str>,
    Result<TagsConfiguration, tree_sitter_tags::Error>,
)> {
    vec![
        (
            vec!["js", "jsx"],
            get_tags_config(
                tree_sitter_javascript::LANGUAGE.into(),
                tree_sitter_javascript::TAGS_QUERY,
                "javascript",
            ),
        ),
        (
            vec!["rb"],
            get_tags_config(
                tree_sitter_ruby::LANGUAGE.into(),
                tree_sitter_ruby::TAGS_QUERY,
                "ruby",
            ),
        ),
        (
            vec!["py", "pyw"],
            get_tags_config(
                tree_sitter_python::LANGUAGE.into(),
                tree_sitter_python::TAGS_QUERY,
                "python",
            ),
        ),
        (
            vec!["c", "h", "i"],
            get_tags_config(
                tree_sitter_c::LANGUAGE.into(),
                tree_sitter_c::TAGS_QUERY,
                "c",
            ),
        ),
        (
            vec![
                "cc", "cpp", "CPP", "cxx", "c++", "cp", "C", "cppm", "ixx", "ii", "H", "hh", "hpp",
                "HPP", "hxx", "h++", "tcc",
            ],
            get_tags_config(
                tree_sitter_cpp::LANGUAGE.into(),
                tree_sitter_cpp::TAGS_QUERY,
                "c++",
            ),
        ),
        (
            vec!["java"],
            get_tags_config(
                tree_sitter_java::LANGUAGE.into(),
                tree_sitter_java::TAGS_QUERY,
                "java",
            ),
        ),
        (
            vec!["ml"],
            get_tags_config(
                tree_sitter_ocaml::LANGUAGE_OCAML.into(),
                tree_sitter_ocaml::TAGS_QUERY,
                "ocaml",
            ),
        ),
        (
            vec!["php"],
            get_tags_config(
                tree_sitter_php::LANGUAGE_PHP.into(),
                tree_sitter_php::TAGS_QUERY,
                "php",
            ),
        ),
        (
            vec!["ts", "tsx"],
            get_tags_config(
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                tree_sitter_typescript::TAGS_QUERY,
                "typescript",
            ),
        ),
        (
            vec!["ex"],
            get_tags_config(
                tree_sitter_elixir::LANGUAGE.into(),
                tree_sitter_elixir::TAGS_QUERY,
                "elixir",
            ),
        ),
        (
            vec!["lua"],
            get_tags_config(
                tree_sitter_lua::LANGUAGE.into(),
                tree_sitter_lua::TAGS_QUERY,
                "lua",
            ),
        ),
        (
            vec!["cs"],
            get_tags_config(
                tree_sitter_c_sharp::LANGUAGE.into(),
                queries::C_SHARP_TAGS_QUERY,
                "c#",
            ),
        ),
        (
            vec!["sh", "bash"],
            get_tags_config(
                tree_sitter_bash::LANGUAGE.into(),
                queries::BASH_TAGS_QUERY,
                "bash",
            ),
        ),
        (
            vec!["scala"],
            get_tags_config(
                tree_sitter_scala::LANGUAGE.into(),
                queries::SCALA_TAGS_QUERY,
                "scala",
            ),
        ),
        (
            vec!["jl"],
            get_tags_config(
                tree_sitter_julia::LANGUAGE.into(),
                queries::JULIA_TAGS_QUERY,
                "julia",
            ),
        ),
    ]
}
