//! Ocaml parser module with optimized C compilation
//!
//! This module provides the same interface as tree-sitter-ocaml crate
//! but compiles the C parser code directly with aggressive optimizations.

use tree_sitter_language::LanguageFn;

// External C function from our compiled parser
#[link(name = "tree_sitter_ocaml", kind = "static")]
extern "C" {
    fn tree_sitter_ocaml() -> *const ();
}

pub const LANGUAGE_OCAML: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_ocaml) };

pub const TAGS_QUERY: &str = include_str!("./ocaml/queries/tags.scm");
