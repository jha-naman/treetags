//! Terraform tags generated from the external HCL grammar.
use super::common::{
    cursor::line_of,
    scope_walker::{walk_tree, WalkContext},
    tree_walker::{generate_tags_with_config, Context},
};
use super::TagKindConfig;
use crate::for_each_child;
use crate::tag::{ExtensionFields, Tag};
use tree_sitter::{Parser as TsParser, TreeCursor};

pub(crate) const LANG_NAME: &str = "terraform";
pub(crate) const LANG_EXTENSIONS: &[&str] = &["tf", "tfvars"];
pub(crate) const KIND_DEFAULTS: &[(&[&str], &str)] = &[
    (&["d", "data"], "d"),
    (&["l", "local"], "l"),
    (&["m", "module"], "m"),
    (&["o", "output"], "o"),
    (&["p", "provider"], "p"),
    (&["r", "resource"], "r"),
    (&["v", "variable"], "v"),
];
pub(crate) const KIND_OPTIONALS: &[(&[&str], &str)] = &[];

struct TerraformWalker<'a> {
    base: Context<'a>,
    block_stack: Vec<bool>,
    tfvars: bool,
}

impl WalkContext for TerraformWalker<'_> {
    fn process_node(&mut self, cursor: &mut TreeCursor) -> bool {
        match cursor.node().kind() {
            "block" => self.process_block(cursor),
            "attribute" => {
                self.process_attribute(cursor);
                false
            }
            _ => false,
        }
    }

    fn pop_scope(&mut self) {
        self.block_stack.pop();
    }
}

impl TerraformWalker<'_> {
    fn process_block(&mut self, cursor: &mut TreeCursor) -> bool {
        let mut block_type = None;
        let mut labels = Vec::new();
        for_each_child!(cursor, {
            let child = cursor.node();
            match child.kind() {
                "identifier" if block_type.is_none() => {
                    block_type = Some(self.base.node_text(&child).to_string());
                }
                "string_lit" => {
                    let text = self.base.node_text(&child);
                    labels.push((unquote(text).to_string(), line_of(child)));
                }
                _ => {}
            }
        });

        let kind = match block_type.as_deref() {
            Some("data") => Some("d"),
            Some("module") => Some("m"),
            Some("output") => Some("o"),
            Some("provider") => Some("p"),
            Some("resource") => Some("r"),
            Some("variable") => Some("v"),
            _ => None,
        };
        if let Some(kind) = kind {
            let index = usize::from(matches!(kind, "d" | "r"));
            if let Some((name, line)) = labels.get(index) {
                self.emit_tag(name.clone(), *line, kind, "def");
            }
        }

        self.block_stack
            .push(block_type.as_deref() == Some("locals"));
        true
    }

    fn process_attribute(&mut self, cursor: &mut TreeCursor) {
        let (kind, role) = if self.tfvars
            && self.base.user_config.extras_config.reference
            && self.block_stack.is_empty()
        {
            ("v", "assigned")
        } else if self.block_stack.last() == Some(&true) {
            ("l", "def")
        } else {
            return;
        };
        let mut name_line = None;
        for_each_child!(cursor, {
            let child = cursor.node();
            if child.kind() == "identifier" {
                name_line = Some((self.base.node_text(&child).to_string(), line_of(child)));
                break;
            }
        });
        if let Some((name, line)) = name_line {
            self.emit_tag(name, line, kind, role);
        }
    }

    fn emit_tag(&mut self, name: String, line: u32, kind: &'static str, role: &'static str) {
        if name.is_empty() || name == "_" || !self.base.tag_config.is_kind_enabled(kind) {
            return;
        }
        let config = &self.base.user_config.fields_config;
        let mut fields = ExtensionFields::new();
        if config.is_field_enabled("kind") {
            fields.insert("kind", kind);
        }
        if config.is_field_enabled("line") {
            fields.insert("line", line.to_string());
        }
        if config.is_field_enabled("language") {
            fields.insert("language", "Terraform");
        }
        if config.is_field_enabled("roles") {
            fields.insert("roles", role);
        }
        self.base.tags.push(Tag {
            name,
            file_name: self.base.file_name.clone(),
            address: Tag::address_from_line(
                self.base
                    .lines
                    .get(line.saturating_sub(1) as usize)
                    .copied()
                    .unwrap_or(b""),
            ),
            kind: Some(kind.into()),
            extension_fields: (!fields.is_empty()).then_some(fields),
        });
    }
}

fn unquote(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(text)
}

pub(crate) fn generate(
    parser: &mut TsParser,
    language: tree_sitter::Language,
    source: &[u8],
    path: &str,
    kinds: &TagKindConfig,
    config: &crate::config::Config,
) -> Option<Vec<Tag>> {
    generate_tags_with_config(
        parser,
        language,
        source,
        path,
        |source_code, lines, cursor, tags| {
            let mut walker = TerraformWalker {
                base: Context {
                    source_code,
                    lines,
                    file_name: path.into(),
                    tags,
                    tag_config: kinds,
                    user_config: config,
                },
                block_stack: Vec::new(),
                tfvars: path.ends_with(".tfvars"),
            };
            walk_tree(cursor, &mut walker);
        },
    )
}
