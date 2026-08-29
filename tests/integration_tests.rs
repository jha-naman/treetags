//! Integration tests that include generated test cases

#[path = "helpers/mod.rs"]
mod helpers;

// Compile the WASM plugins before libtest starts running test functions.
#[ctor::ctor(unsafe)]
fn build_test_plugins() {
    helpers::plugin_builder::test_plugins_dir();
}

// Include all generated tests
include!(concat!(env!("OUT_DIR"), "/generated_tests/all_tests.rs"));
