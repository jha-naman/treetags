/// Directory containing the WASM plugins compiled before the test executable
/// starts. An empty string means plugin compilation was unavailable.
pub fn test_plugins_dir() -> &'static str {
    env!("TREETAGS_TEST_PLUGINS_DIR")
}
