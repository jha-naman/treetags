//! Rust parser module with optimized C compilation
//! 
//! This module provides the same interface as tree-sitter-rust crate
//! but compiles the C parser code directly with aggressive optimizations.

use tree_sitter::Language;

// External C function from our compiled parser
#[link(name = "tree_sitter_rust", kind = "static")]
extern "C" {
    fn tree_sitter_rust() -> *const tree_sitter::ffi::TSLanguage;
}

pub fn language() -> Language {
    unsafe { Language::from_raw(tree_sitter_rust()) }
}

