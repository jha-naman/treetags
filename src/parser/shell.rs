//! Bash definitions and script references from the bundled grammar.
use super::common::{
    cursor::{field_child, line_of, node_text},
    scope_walker::{walk_tree, WalkContext},
    tree_walker::{generate_tags_with_config, Context},
};
use super::TagKindConfig;
use crate::for_each_child;
use crate::tag::{ExtensionFields, Tag};
use tree_sitter::{Node, Parser as TsParser, TreeCursor};

pub(crate) const KIND_DEFAULTS: &[(&[&str], &str)] = &[
    (&["a", "alias"], "a"),
    (&["f", "function"], "f"),
    (&["h", "heredoc"], "h"),
    (&["s", "script"], "s"),
];

struct Walker<'a> {
    base: Context<'a>,
}

fn unquote(raw: &str) -> &str {
    raw.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| raw.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(raw)
}

impl Walker<'_> {
    fn emit(
        &mut self,
        name: String,
        kind: &'static str,
        node: Node,
        role: &'static str,
        end: Option<u32>,
    ) {
        if name.is_empty() || !self.base.tag_config.is_kind_enabled(kind) {
            return;
        }
        if role != "def" && !self.base.user_config.extras_config.reference {
            return;
        }
        let config = &self.base.user_config.fields_config;
        let mut fields = ExtensionFields::new();
        if config.is_field_enabled("kind") {
            fields.insert("kind", kind);
        }
        if config.is_field_enabled("line") {
            fields.insert("line", line_of(node).to_string());
        }
        if config.is_field_enabled("language") {
            fields.insert("language", "Sh");
        }
        if config.is_field_enabled("roles") {
            fields.insert("roles", role);
        }
        if config.is_field_enabled("end") {
            if let Some(end) = end {
                fields.insert("end", end.to_string());
            }
        }
        let line = line_of(node) as usize;
        self.base.tags.push(Tag {
            name,
            file_name: self.base.file_name.clone(),
            address: Tag::address_from_line(self.base.lines.get(line - 1).copied().unwrap_or(b"")),
            kind: Some(kind.into()),
            extension_fields: (!fields.is_empty()).then_some(fields),
        });
    }

    fn command(&mut self, cursor: &mut TreeCursor) {
        let Some(command_name) = field_child(cursor, "name") else {
            return;
        };
        let command = node_text(command_name, self.base.source_code.as_bytes());
        if !matches!(command, "alias" | "source" | ".") {
            return;
        }
        let mut argument = None;
        for_each_child!(cursor, {
            if cursor.field_name() == Some("argument") {
                argument = Some(cursor.node());
                break;
            }
        });
        let Some(argument) = argument else { return };
        let raw = node_text(argument, self.base.source_code.as_bytes());
        match command {
            "alias" => {
                if let Some((name, _)) = unquote(raw).split_once('=') {
                    self.emit(name.to_owned(), "a", cursor.node(), "def", None);
                }
            }
            "source" | "." => {
                if !raw.contains('$') && !raw.contains('`') {
                    self.emit(unquote(raw).to_owned(), "s", cursor.node(), "loaded", None);
                }
            }
            _ => unreachable!(),
        }
    }

    fn heredoc(&mut self, cursor: &mut TreeCursor) {
        let mut start = None;
        let mut end = None;
        for_each_child!(cursor, {
            match cursor.node().kind() {
                "heredoc_start" => start = Some(cursor.node()),
                "heredoc_end" => end = Some(cursor.node()),
                _ => {}
            }
        });
        let Some(start) = start else { return };
        let name = unquote(node_text(start, self.base.source_code.as_bytes())).to_owned();
        // The redirect starts at `<<`; the containing command owns the source line.
        let declaration = cursor.node().parent().unwrap_or(cursor.node());
        self.emit(name.clone(), "h", declaration, "def", end.map(line_of));
        if let Some(end) = end {
            self.emit(name, "h", end, "endmarker", None);
        }
    }
}

impl WalkContext for Walker<'_> {
    fn process_node(&mut self, cursor: &mut TreeCursor) -> bool {
        match cursor.node().kind() {
            "function_definition" => {
                if let Some(name) = field_child(cursor, "name") {
                    self.emit(
                        node_text(name, self.base.source_code.as_bytes()).to_owned(),
                        "f",
                        cursor.node(),
                        "def",
                        None,
                    );
                }
            }
            "command" => self.command(cursor),
            "heredoc_redirect" => self.heredoc(cursor),
            _ => {}
        }
        false
    }

    fn pop_scope(&mut self) {}
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
            let mut walker = Walker {
                base: Context {
                    source_code,
                    lines,
                    file_name: path.into(),
                    tags,
                    tag_config: kinds,
                    user_config: config,
                },
            };
            walk_tree(cursor, &mut walker);
        },
    )
}
