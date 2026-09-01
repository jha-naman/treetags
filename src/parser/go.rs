use super::TagKindConfig;
use crate::tag;

#[path = "go_oracle.rs"]
pub(crate) mod oracle;

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
    parser: &mut tree_sitter::Parser,
    code: &[u8],
    path: &str,
    kinds: &TagKindConfig,
    config: &crate::config::Config,
) -> Option<Vec<tag::Tag>> {
    #[cfg(not(feature = "linear-go"))]
    return oracle::generate(parser, code, path, kinds, config);
    #[cfg(feature = "linear-go")]
    {
        let _ = parser;
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
}
