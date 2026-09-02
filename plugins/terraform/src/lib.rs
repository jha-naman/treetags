wit_bindgen::generate!({
    world: "plugin-world",
    path: "../../wit",
});

use exports::treetags::plugin::plugin::{Guest, Request, Tag};
use tree_sitter::{Parser as TsParser, TreeCursor};
use treetags_plugin_common::{
    for_each_child, line_of, node_text, walk_tree, TagKindConfig, WalkContext,
};

struct TerraformPlugin;

impl Guest for TerraformPlugin {
    fn generate(req: Request, source: Vec<u8>) -> Result<Vec<Tag>, String> {
        let mut parser = TsParser::new();
        let language: tree_sitter::Language = tree_sitter_hcl::LANGUAGE.into();
        parser
            .set_language(&language)
            .map_err(|error| format!("set_language: {error}"))?;
        generate_tags(&mut parser, &req, &source)
    }
}

export!(TerraformPlugin);

// This table must stay in sync with plugin.toml.
const TERRAFORM_DEFAULT_KINDS: &[(&[&str], &str)] = &[
    (&["d", "data"], "d"),
    (&["l", "local"], "l"),
    (&["m", "module"], "m"),
    (&["o", "output"], "o"),
    (&["p", "provider"], "p"),
    (&["r", "resource"], "r"),
    (&["v", "variable"], "v"),
];

struct TerraformWalker<'src> {
    source: &'src [u8],
    kinds: TagKindConfig,
    block_stack: Vec<bool>,
    tfvars: bool,
    reference_tags: bool,
    language_field: bool,
    roles_field: bool,
    tags: Vec<Tag>,
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
        let mut identifiers = Vec::new();
        let mut labels = Vec::new();
        for_each_child!(cursor, {
            let child = cursor.node();
            match child.kind() {
                "identifier" => {
                    identifiers.push((node_text(child, self.source).to_string(), line_of(child)))
                }
                "string_lit" => {
                    labels.push((unquote(node_text(child, self.source)), line_of(child)))
                }
                _ => {}
            }
        });

        let block_type = identifiers
            .first()
            .map(|(name, _)| name.as_str())
            .unwrap_or("");
        let kind = match block_type {
            "data" => Some("d"),
            "module" => Some("m"),
            "output" => Some("o"),
            "provider" => Some("p"),
            "resource" => Some("r"),
            "variable" => Some("v"),
            _ => None,
        };

        if let Some(kind) = kind {
            let label_index = usize::from(matches!(kind, "d" | "r"));
            if self.kinds.is_enabled(kind) {
                if let Some((name, line)) = labels.get(label_index) {
                    self.tags
                        .push(self.make_tag(name.clone(), *line, kind, "def"));
                }
            }
        }

        self.block_stack.push(block_type == "locals");
        true
    }

    fn process_attribute(&mut self, cursor: &mut TreeCursor) {
        let (kind, role) = if self.tfvars && self.reference_tags && self.block_stack.is_empty() {
            ("v", "assigned")
        } else if self.block_stack.last() == Some(&true) {
            ("l", "def")
        } else {
            return;
        };
        if !self.kinds.is_enabled(kind) {
            return;
        }

        let mut name_line = None;
        for_each_child!(cursor, {
            if cursor.node().kind() == "identifier" {
                let node = cursor.node();
                name_line = Some((node_text(node, self.source).to_string(), line_of(node)));
                break;
            }
        });
        if let Some((name, line)) = name_line {
            self.tags.push(self.make_tag(name, line, kind, role));
        }
    }

    fn make_tag(&self, name: String, line: u32, kind: &str, role: &str) -> Tag {
        let mut extension_fields = Vec::new();
        if self.language_field {
            extension_fields.push(("language".to_string(), "Terraform".to_string()));
        }
        if self.roles_field {
            extension_fields.push(("roles".to_string(), role.to_string()));
        }
        Tag {
            name,
            line,
            kind: kind.to_string(),
            end_line: None,
            extension_fields,
        }
    }
}

fn unquote(text: &str) -> String {
    text.strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(text)
        .to_string()
}

fn option_enabled(fields: &str, letter: char, name: &str) -> bool {
    if fields.contains(',') || fields.contains('+') || fields.contains('-') {
        let mut enabled = false;
        for field in fields.split(',').map(str::trim) {
            let (enable, field) = match field.as_bytes().first() {
                Some(b'+') => (true, &field[1..]),
                Some(b'-') => (false, &field[1..]),
                _ => (true, field),
            };
            if field == letter.to_string() || field == name {
                enabled = enable;
            }
        }
        enabled
    } else {
        fields.contains(letter)
    }
}

fn field_enabled(fields: &str, letter: char, name: &str) -> bool {
    option_enabled(fields, letter, name)
}

fn generate_tags(parser: &mut TsParser, req: &Request, source: &[u8]) -> Result<Vec<Tag>, String> {
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "parse failed".to_string())?;
    let mut walker = TerraformWalker {
        source,
        kinds: TagKindConfig::parse(&req.kinds, TERRAFORM_DEFAULT_KINDS, &[]),
        block_stack: Vec::new(),
        tfvars: req.file_path.ends_with(".tfvars"),
        reference_tags: option_enabled(&req.extras, 'r', "reference"),
        language_field: field_enabled(&req.fields, 'l', "language"),
        roles_field: field_enabled(&req.fields, 'r', "roles"),
        tags: Vec::new(),
    };
    let mut cursor = tree.walk();
    if cursor.goto_first_child() {
        walk_tree(&mut cursor, &mut walker);
    }
    Ok(walker.tags)
}
