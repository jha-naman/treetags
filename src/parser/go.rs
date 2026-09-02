use super::TagKindConfig;
use crate::tag;

pub(crate) const LANG_NAME: &str = "go";
pub(crate) const LANG_EXTENSIONS: &[&str] = &["go"];
pub(crate) const KIND_DEFAULTS: &[(&[&str], &str)] = &[
    (&["p", "package"], "p"),
    (&["f", "function"], "f"),
    (&["c", "constant"], "c"),
    (&["t", "type"], "t"),
    (&["v", "variable"], "v"),
    (&["s", "struct"], "s"),
    (&["i", "interface"], "i"),
    (&["m", "member"], "m"),
    (&["M", "anonymous"], "M"),
    (&["n", "method"], "n"),
    (&["P", "import"], "P"),
    (&["a", "alias"], "a"),
];
pub(crate) const KIND_OPTIONALS: &[(&[&str], &str)] = &[];

pub(crate) fn generate(
    _parser: &mut tree_sitter::Parser,
    code: &[u8],
    path: &str,
    kinds: &TagKindConfig,
    config: &crate::config::Config,
) -> Option<Vec<tag::Tag>> {
    let source = match std::str::from_utf8(code) {
        Ok(source) => source,
        Err(_) => {
            eprintln!("Warning: Input for {path} is not valid UTF-8, skipping.");
            return None;
        }
    };
    super::go_hooks::generate(
        source,
        path,
        super::linear::HookOptions::from_config(kinds, config),
    )
    .map_err(|error| eprintln!("Warning: Failed to scan {path}: {error}"))
    .ok()
}
