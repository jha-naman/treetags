use std::process;

use crate::config::Config;
use crate::language_parser::LanguageParserRegistry;
use crate::parser::KindInfo;

/// Handle the `--list-kinds [LANG]` command.
///
/// When `lang` is `None`, all known languages are listed.
/// When `lang` is `Some(name)`, the table for that language is printed.
///
/// Uses manifest-scanning for plugins (no WASM loading) to stay fast.
pub fn handle(lang: Option<&str>, config: &Config) {
    // Build a lightweight registry: builtin parsers are trivially cheap to
    // construct; WASM plugins are only manifest-scanned (no JIT compilation)
    // because we only need kind metadata here.
    let registry = build_listing_registry(config);

    match lang {
        None => list_all(&registry),
        Some(l) => list_one(l, &registry),
    }
}

/// Builds a registry suitable for kinds listing.
/// For WASM plugins this only reads manifests — no WASM is loaded.
fn build_listing_registry(config: &Config) -> LanguageParserRegistry {
    LanguageParserRegistry::new(config)
}

fn list_all(registry: &LanguageParserRegistry) {
    let mut parsers: Vec<_> = registry
        .all_languages()
        .filter(|lp| !lp.kinds().is_empty()) // skip query fallback parsers
        .collect();
    parsers.sort_unstable_by_key(|lp| lp.language_name());

    for lp in parsers {
        println!("{}", lp.language_name());
        for k in lp.kinds() {
            if k.default {
                println!("    {}  {}", k.letter, k.name);
            } else {
                println!("    {}  {} [off]", k.letter, k.name);
            }
        }
    }
}

fn list_one(lang: &str, registry: &LanguageParserRegistry) {
    if let Some(lp) = registry.for_language(lang) {
        print_kinds_table(&lp.kinds());
        return;
    }

    eprintln!(
        "treetags: unknown language '{}'; use --list-kinds to see available languages",
        lang
    );
    process::exit(1);
}

fn print_kinds_table(kinds: &[KindInfo]) {
    println!("{:<8} {:<24} {}", "#LETTER", "NAME", "ENABLED");
    for k in kinds {
        println!(
            "{:<8} {:<24} {}",
            k.letter,
            k.name,
            if k.default { "yes" } else { "no" }
        );
    }
}
