//! Integration tests that include generated test cases

#[path = "helpers/mod.rs"]
mod helpers;

// Include all generated tests
include!(concat!(env!("OUT_DIR"), "/generated_tests/all_tests.rs"));
