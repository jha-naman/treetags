//! Official tag-style capabilities; this path never loads a grammar or plugin.

use crate::builtin_langs::OFFICIAL_LANGUAGES;
use crate::config::{tag_styles::official_language, Config};

pub fn handle(language: &str, config: &Config) -> Result<(), String> {
    let mut descriptors = if language.is_empty() {
        OFFICIAL_LANGUAGES.iter().collect::<Vec<_>>()
    } else {
        vec![official_language(language)
            .ok_or_else(|| format!("unknown official language '{language}'"))?]
    };
    descriptors.sort_by_key(|d| d.lang);
    println!("# Official tag styles; installed tag plugins retain precedence.");
    println!("LANGUAGE\tGRAMMAR\tAVAILABLE\tPREFERRED\tEFFECTIVE\tREASON");
    for desc in descriptors {
        let selection = config.tag_preferences.select(desc);
        let mut available = Vec::new();
        if desc.query.is_some() {
            available.push("basic");
        }
        if desc.generate_fn.is_some() {
            available.push("with_extension_fields");
        }
        let reason = if selection.preferred == selection.effective {
            "preferred style available".to_owned()
        } else {
            format!(
                "{} not implemented; using available style",
                selection.preferred.label()
            )
        };
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            desc.lang,
            if desc.grammar.external().is_some() {
                "wasm"
            } else {
                "bundled"
            },
            available.join(","),
            selection.preferred.label(),
            selection.effective.label(),
            reason
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readme_capabilities_match_catalog() {
        let mut descriptors = OFFICIAL_LANGUAGES.iter().collect::<Vec<_>>();
        descriptors.sort_by_key(|d| d.lang);
        let mut table = String::from(
            "| Language | Grammar | Basic | With extension fields |\n| --- | --- | --- | --- |\n",
        );
        for desc in descriptors {
            table.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                desc.lang,
                if desc.grammar.external().is_some() {
                    "wasm"
                } else {
                    "bundled"
                },
                if desc.query.is_some() { "yes" } else { "—" },
                if desc.generate_fn.is_some() {
                    "yes"
                } else {
                    "—"
                }
            ));
        }
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
