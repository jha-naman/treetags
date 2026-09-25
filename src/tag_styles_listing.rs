//! Official tag-style capabilities; this path never loads a grammar or plugin.

use std::fmt::Write;

use crate::builtin_langs::{LanguageDescriptor, OFFICIAL_LANGUAGES};
use crate::config::{tag_styles::official_language, tag_styles::TagPreferences, Config};

fn render_table(descriptors: &[&LanguageDescriptor], preferences: &TagPreferences) -> String {
    let mut descriptors = descriptors.to_vec();
    descriptors.sort_by_key(|desc| desc.lang);

    let mut table = String::new();
    table.push_str("# Official tag styles; installed tag plugins retain precedence.\n");
    table.push_str("LANGUAGE\tGRAMMAR\tAVAILABLE\tPREFERRED\tEFFECTIVE\tREASON\n");

    for desc in descriptors {
        let grammar = if desc.grammar.external().is_some() {
            "wasm"
        } else {
            "bundled"
        };
        let basic = desc.query.is_some();
        let extended = desc.generate_fn.is_some();
        let selection = preferences.select(desc);
        let available = match (basic, extended) {
            (true, true) => "basic,extended",
            (true, false) => "basic",
            (false, true) => "extended",
            (false, false) => "",
        };
        let reason = if selection.preferred == selection.effective {
            "preferred style available".to_owned()
        } else {
            format!(
                "{} not implemented; using available style",
                selection.preferred.label()
            )
        };
        writeln!(
            table,
            "{}\t{}\t{}\t{}\t{}\t{}",
            desc.lang,
            grammar,
            available,
            selection.preferred.label(),
            selection.effective.label(),
            reason
        )
        .unwrap();
    }
    table
}

pub fn handle(language: &str, config: &Config) -> Result<(), String> {
    let descriptors = if language.is_empty() {
        OFFICIAL_LANGUAGES.iter().collect::<Vec<_>>()
    } else {
        vec![official_language(language)
            .ok_or_else(|| format!("unknown official language '{language}'"))?]
    };
    print!("{}", render_table(&descriptors, &config.tag_preferences));
    Ok(())
}
