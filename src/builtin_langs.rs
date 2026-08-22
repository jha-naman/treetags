use crate::parser::{cpp, generated, go, js, objective_c, python, rust, typescript, TagKindConfig};
use crate::tag::Tag;

/// Function pointer type for builtin language tag generators.
pub(crate) type TreeSitterGenerateFn = fn(
    &mut tree_sitter::Parser,
    &[u8],
    &str,
    &TagKindConfig,
    &crate::config::Config,
) -> Option<Vec<Tag>>;
pub(crate) type NativeGenerateFn =
    fn(&[u8], &str, &TagKindConfig, &crate::config::Config) -> Option<Vec<Tag>>;

#[derive(Clone, Copy)]
pub(crate) enum BuiltinGenerateFn {
    TreeSitter(TreeSitterGenerateFn),
    Native(NativeGenerateFn),
}

impl BuiltinGenerateFn {
    pub(crate) fn generate(
        self,
        parser: &mut tree_sitter::Parser,
        code: &[u8],
        path: &str,
        kinds: &TagKindConfig,
        config: &crate::config::Config,
    ) -> Option<Vec<Tag>> {
        match self {
            Self::TreeSitter(generate) => generate(parser, code, path, kinds, config),
            Self::Native(generate) => generate(code, path, kinds, config),
        }
    }
}

/// Full descriptor for a builtin language: name, extensions, kind mappings, generate fn.
pub(crate) struct BuiltinLangDesc {
    pub lang: &'static str,
    /// Alternate names accepted by `--language-force` (and future langmap options),
    /// in addition to `lang`. Case-insensitive at lookup time.
    pub aliases: &'static [&'static str],
    pub extensions: &'static [&'static str],
    /// `fnmatch`-style filename globs (matched against the basename) that select
    /// this language, e.g. `SConstruct` or `*.gyp`. Enables matching files that
    /// have no distinguishing extension.
    pub patterns: &'static [&'static str],
    /// Interpreter names matched against a `#!` shebang line, e.g. `python3`.
    pub interpreters: &'static [&'static str],
    pub kind_defaults: &'static [(&'static [&'static str], &'static str)],
    pub kind_optionals: &'static [(&'static [&'static str], &'static str)],
    pub generate_fn: BuiltinGenerateFn,
}

/// All builtin languages. Priority in tag generation follows array order.
/// Adding a new builtin language requires exactly one new entry here.
pub(crate) static BUILTIN_LANG_DESCRIPTORS: &[BuiltinLangDesc] = &[
    BuiltinLangDesc {
        lang: rust::LANG_NAME,
        aliases: &[],
        extensions: rust::LANG_EXTENSIONS,
        patterns: &[],
        interpreters: &[],
        kind_defaults: rust::KIND_DEFAULTS,
        kind_optionals: rust::KIND_OPTIONALS,
        generate_fn: BuiltinGenerateFn::TreeSitter(rust::generate),
    },
    BuiltinLangDesc {
        lang: go::LANG_NAME,
        aliases: &["golang"],
        extensions: go::LANG_EXTENSIONS,
        patterns: &[],
        interpreters: &[],
        kind_defaults: go::KIND_DEFAULTS,
        kind_optionals: go::KIND_OPTIONALS,
        generate_fn: BuiltinGenerateFn::TreeSitter(go::generate),
    },
    BuiltinLangDesc {
        lang: cpp::LANG_NAME,
        aliases: &["cpp", "cxx", "cplusplus"],
        extensions: cpp::LANG_EXTENSIONS,
        patterns: &[],
        interpreters: &[],
        kind_defaults: generated::cpp::KIND_DEFAULTS,
        kind_optionals: generated::cpp::KIND_OPTIONALS,
        generate_fn: BuiltinGenerateFn::Native(generated::cpp::generate),
    },
    // C reuses the C++ parser but is a distinct language with its own kind table.
    BuiltinLangDesc {
        lang: cpp::C_LANG_NAME,
        aliases: &[],
        extensions: cpp::C_LANG_EXTENSIONS,
        patterns: &[],
        interpreters: &[],
        kind_defaults: generated::c::KIND_DEFAULTS,
        kind_optionals: generated::c::KIND_OPTIONALS,
        generate_fn: BuiltinGenerateFn::Native(generated::c::generate),
    },
    BuiltinLangDesc {
        lang: objective_c::LANG_NAME,
        aliases: objective_c::LANG_ALIASES,
        extensions: objective_c::LANG_EXTENSIONS,
        patterns: &[],
        interpreters: &[],
        kind_defaults: generated::objective_c::KIND_DEFAULTS,
        kind_optionals: generated::objective_c::KIND_OPTIONALS,
        generate_fn: BuiltinGenerateFn::Native(generated::objective_c::generate),
    },
    BuiltinLangDesc {
        lang: js::LANG_NAME,
        aliases: &["js"],
        extensions: js::LANG_EXTENSIONS,
        patterns: &[],
        interpreters: &["node", "nodejs"],
        kind_defaults: js::KIND_DEFAULTS,
        kind_optionals: js::KIND_OPTIONALS,
        generate_fn: BuiltinGenerateFn::TreeSitter(js::generate),
    },
    BuiltinLangDesc {
        lang: python::LANG_NAME,
        aliases: &[],
        extensions: python::LANG_EXTENSIONS,
        patterns: &["SConstruct", "SConscript", "wscript"],
        interpreters: &["python", "python2", "python3"],
        kind_defaults: python::KIND_DEFAULTS,
        kind_optionals: python::KIND_OPTIONALS,
        generate_fn: BuiltinGenerateFn::TreeSitter(python::generate),
    },
    BuiltinLangDesc {
        lang: typescript::LANG_NAME,
        aliases: &["ts"],
        extensions: typescript::LANG_EXTENSIONS,
        patterns: &[],
        interpreters: &[],
        kind_defaults: typescript::KIND_DEFAULTS,
        kind_optionals: typescript::KIND_OPTIONALS,
        generate_fn: BuiltinGenerateFn::TreeSitter(typescript::generate),
    },
];
