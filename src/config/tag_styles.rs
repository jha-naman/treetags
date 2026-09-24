//! Output preferences are independent of language resolution and grammar delivery.

use clap::ValueEnum;
use serde::Deserialize;

use crate::builtin_langs::{LanguageDescriptor, OFFICIAL_LANGUAGES};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum TagStyle {
    #[default]
    Basic,
    #[value(name = "with_extension_fields")]
    WithExtensionFields,
}

impl TagStyle {
    pub fn label(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::WithExtensionFields => "with_extension_fields",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TagPreferences {
    pub default: TagStyle,
    pub basic: Vec<String>,
    pub with_extension_fields: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TagSelection {
    pub preferred: TagStyle,
    pub effective: TagStyle,
}

impl TagPreferences {
    /// CLI values already include the lower-priority --options file. None means
    /// inherit TOML; Some("") deliberately clears an inherited language list.
    pub fn merge(
        mut self,
        default: Option<TagStyle>,
        basic: Option<&str>,
        with_extension_fields: Option<&str>,
    ) -> Result<Self, String> {
        if let Some(default) = default {
            self.default = default;
        }
        if let Some(list) = basic {
            self.basic = split_list(list);
        }
        if let Some(list) = with_extension_fields {
            self.with_extension_fields = split_list(list);
        }
        normalize(&mut self.basic)?;
        normalize(&mut self.with_extension_fields)?;
        for language in &self.basic {
            if self.with_extension_fields.contains(language) {
                return Err(format!(
                    "language '{language}' appears in both basic and with_extension_fields tag lists"
                ));
            }
        }
        Ok(self)
    }

    pub fn select(&self, language: &LanguageDescriptor) -> TagSelection {
        let preferred = if self.basic.iter().any(|name| name == language.lang) {
            TagStyle::Basic
        } else if self
            .with_extension_fields
            .iter()
            .any(|name| name == language.lang)
        {
            TagStyle::WithExtensionFields
        } else {
            self.default
        };
        let effective = match preferred {
            TagStyle::Basic if language.query.is_none() => TagStyle::WithExtensionFields,
            TagStyle::WithExtensionFields if language.generate_fn.is_none() => TagStyle::Basic,
            style => style,
        };
        TagSelection {
            preferred,
            effective,
        }
    }
}

pub fn official_language(name: &str) -> Option<&'static LanguageDescriptor> {
    let name = name.trim();
    // Canonical names take priority over aliases, as in language resolution.
    OFFICIAL_LANGUAGES
        .iter()
        .find(|d| d.lang.eq_ignore_ascii_case(name))
        .or_else(|| {
            OFFICIAL_LANGUAGES.iter().find(|d| {
                d.aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(name))
            })
        })
}

fn split_list(list: &str) -> Vec<String> {
    if list.trim().is_empty() {
        Vec::new()
    } else {
        list.split(',').map(str::to_owned).collect()
    }
}

fn normalize(names: &mut Vec<String>) -> Result<(), String> {
    for name in names.iter_mut() {
        let language = official_language(name).ok_or_else(|| {
            format!("unknown official language '{name}' in tag style preferences")
        })?;
        *name = language.lang.to_owned();
    }
    names.sort();
    names.dedup();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_covers_both_single_styles_and_dual_capabilities() {
        let rust = official_language("rust").unwrap();
        let scala = official_language("scala").unwrap();
        let basic = TagPreferences::default();
        assert_eq!(
            basic.select(rust),
            TagSelection {
                preferred: TagStyle::Basic,
                effective: TagStyle::Basic,
            }
        );
        assert_eq!(basic.select(scala).effective, TagStyle::Basic);
        let rich = basic
            .clone()
            .merge(Some(TagStyle::WithExtensionFields), None, None)
            .unwrap();
        assert_eq!(
            rich.select(scala),
            TagSelection {
                preferred: TagStyle::WithExtensionFields,
                effective: TagStyle::Basic,
            }
        );
        assert_eq!(rich.select(rust).effective, TagStyle::WithExtensionFields);
        let overridden = rich.merge(None, Some("rust"), None).unwrap();
        assert_eq!(overridden.select(rust).effective, TagStyle::Basic);
    }

    #[test]
    fn list_replacement_clearing_aliases_and_conflicts() {
        let toml = TagPreferences {
            basic: vec!["ruby".into()],
            with_extension_fields: vec!["rust".into()],
            ..Default::default()
        };
        let merged = toml
            .clone()
            .merge(None, Some(" GoLang , GO , csharp "), Some(""))
            .unwrap();
        assert_eq!(merged.basic, vec!["c#", "go"]);
        assert!(merged.with_extension_fields.is_empty());
        assert!(toml.merge(None, Some("RUST"), None).is_err());
        assert!(TagPreferences::default()
            .merge(None, Some("missing"), None)
            .is_err());
        assert!(TagPreferences::default()
            .merge(None, Some("rust,,go"), None)
            .is_err());
    }

    #[test]
    fn catalog_has_one_registration_per_language_and_a_capability() {
        let mut names = std::collections::HashSet::new();
        for desc in OFFICIAL_LANGUAGES {
            assert!(names.insert(desc.lang), "duplicate {}", desc.lang);
            assert!(
                desc.query.is_some() || desc.generate_fn.is_some(),
                "{} has no implementation",
                desc.lang
            );
            assert!(
                !desc.extensions.is_empty(),
                "{} needs a compatibility extension",
                desc.lang
            );
        }
        assert_eq!(names.len(), 23);
    }
}
