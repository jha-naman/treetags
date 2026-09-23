//! Official tag-style capabilities; this path never loads a grammar or plugin.

use std::fmt::Write;

use crate::builtin_langs::{LanguageDescriptor, OFFICIAL_LANGUAGES};
use crate::config::{tag_styles::official_language, tag_styles::TagPreferences, Config};

#[derive(Clone, Copy)]
enum TableFormat {
    Tsv,
    #[cfg(test)]
    Markdown,
}

fn render_table(
    descriptors: &[&LanguageDescriptor],
    preferences: &TagPreferences,
    format: TableFormat,
) -> String {
    let mut descriptors = descriptors.to_vec();
    descriptors.sort_by_key(|desc| desc.lang);

    let mut table = String::new();
    match format {
        TableFormat::Tsv => {
            table.push_str("# Official tag styles; installed tag plugins retain precedence.\n");
            table.push_str("LANGUAGE\tGRAMMAR\tAVAILABLE\tPREFERRED\tEFFECTIVE\tREASON\n");
        }
        #[cfg(test)]
        TableFormat::Markdown => {
            table.push_str("| Language | Grammar | Basic | With extension fields |\n");
            table.push_str("| --- | --- | --- | --- |\n");
        }
    }

    for desc in descriptors {
        let grammar = if desc.grammar.external().is_some() {
            "wasm"
        } else {
            "bundled"
        };
        let basic = desc.query.is_some();
        let with_extension_fields = desc.generate_fn.is_some();
        match format {
            TableFormat::Tsv => {
                let selection = preferences.select(desc);
                let available = match (basic, with_extension_fields) {
                    (true, true) => "basic,with_extension_fields",
                    (true, false) => "basic",
                    (false, true) => "with_extension_fields",
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
            #[cfg(test)]
            TableFormat::Markdown => {
                writeln!(
                    table,
                    "| {} | {} | {} | {} |",
                    desc.lang,
                    grammar,
                    if basic { "yes" } else { "—" },
                    if with_extension_fields { "yes" } else { "—" }
                )
                .unwrap();
            }
        }
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
    print!(
        "{}",
        render_table(&descriptors, &config.tag_preferences, TableFormat::Tsv)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readme_capabilities_match_catalog() {
        let descriptors = OFFICIAL_LANGUAGES.iter().collect::<Vec<_>>();
        let table = render_table(
            &descriptors,
            &TagPreferences::default(),
            TableFormat::Markdown,
        );
        let readme = include_str!("../README.md");
        let documented = readme
            .split("<!-- tag-style-capabilities:start -->\n")
            .nth(1)
            .unwrap()
            .split("<!-- tag-style-capabilities:end -->")
            .next()
            .unwrap();
        assert_eq!(
            documented, table,
            "replace the README capability block with the catalog-generated table"
        );
    }
}
