use crate::parser::{cpp, go, js, python, rust, typescript, TagKindConfig};
use crate::tag::Tag;

/// Function pointer type for builtin language tag generators.
pub(crate) type BuiltinGenerateFn = fn(
    &mut tree_sitter::Parser,
    &[u8],
    &str,
    &TagKindConfig,
    &crate::config::Config,
) -> Option<Vec<Tag>>;

/// Full descriptor for a builtin language: name, extensions, kind mappings, generate fn.
pub(crate) struct BuiltinLangDesc {
    pub lang: &'static str,
    pub extensions: &'static [&'static str],
    pub kind_defaults: &'static [(&'static [&'static str], &'static str)],
    pub kind_optionals: &'static [(&'static [&'static str], &'static str)],
    pub generate_fn: BuiltinGenerateFn,
}

/// All builtin languages. Priority in tag generation follows array order.
/// Adding a new builtin language requires exactly one new entry here.
pub(crate) static BUILTIN_LANG_DESCRIPTORS: &[BuiltinLangDesc] = &[
    BuiltinLangDesc {
        lang: rust::LANG_NAME,
        extensions: rust::LANG_EXTENSIONS,
        kind_defaults: rust::KIND_DEFAULTS,
        kind_optionals: rust::KIND_OPTIONALS,
        generate_fn: rust::generate,
    },
    BuiltinLangDesc {
        lang: go::LANG_NAME,
        extensions: go::LANG_EXTENSIONS,
        kind_defaults: go::KIND_DEFAULTS,
        kind_optionals: go::KIND_OPTIONALS,
        generate_fn: go::generate,
    },
    BuiltinLangDesc {
        lang: cpp::LANG_NAME,
        extensions: cpp::LANG_EXTENSIONS,
        kind_defaults: cpp::KIND_DEFAULTS,
        kind_optionals: cpp::KIND_OPTIONALS,
        generate_fn: cpp::generate,
    },
    // C reuses the C++ parser but is a distinct language with its own kind table.
    BuiltinLangDesc {
        lang: cpp::C_LANG_NAME,
        extensions: cpp::C_LANG_EXTENSIONS,
        kind_defaults: cpp::C_KIND_DEFAULTS,
        kind_optionals: cpp::C_KIND_OPTIONALS,
        generate_fn: cpp::generate,
    },
    BuiltinLangDesc {
        lang: js::LANG_NAME,
        extensions: js::LANG_EXTENSIONS,
        kind_defaults: js::KIND_DEFAULTS,
        kind_optionals: js::KIND_OPTIONALS,
        generate_fn: js::generate,
    },
    BuiltinLangDesc {
        lang: python::LANG_NAME,
        extensions: python::LANG_EXTENSIONS,
        kind_defaults: python::KIND_DEFAULTS,
        kind_optionals: python::KIND_OPTIONALS,
        generate_fn: python::generate,
    },
    BuiltinLangDesc {
        lang: typescript::LANG_NAME,
        extensions: typescript::LANG_EXTENSIONS,
        kind_defaults: typescript::KIND_DEFAULTS,
        kind_optionals: typescript::KIND_OPTIONALS,
        generate_fn: typescript::generate,
    },
];
