use super::Parser;
use std::collections::{HashMap, HashSet};
use tree_sitter::{Node, TreeCursor};

use crate::{split_by_newlines, tag};

// Represents the type of scope for context tracking
#[derive(Debug)]
enum ScopeType {
    Module,
    Struct,
    Enum,
    Union,
    Trait,
    Implementation, // Can store both trait and type info if needed
}

// Configuration for which tag kinds to generate
#[derive(Debug, Clone)]
pub struct TagKindConfig {
    enabled_kinds: HashSet<String>,
    // Cache for optimization - whether we need to traverse certain node types
    needs_traversal_cache: HashMap<String, bool>,
}

impl TagKindConfig {
    /// Create a new configuration with all kinds enabled by default
    pub fn new() -> Self {
        let mut enabled_kinds = HashSet::new();
        // Add all possible tag kinds
        enabled_kinds.insert("n".to_string()); // module
        enabled_kinds.insert("s".to_string()); // struct
        enabled_kinds.insert("g".to_string()); // enum
        enabled_kinds.insert("u".to_string()); // union
        enabled_kinds.insert("i".to_string()); // trait/interface
        enabled_kinds.insert("c".to_string()); // implementation
        enabled_kinds.insert("f".to_string()); // function
        enabled_kinds.insert("P".to_string()); // method/procedure
        enabled_kinds.insert("m".to_string()); // method signature
        enabled_kinds.insert("e".to_string()); // enum variant
        enabled_kinds.insert("T".to_string()); // associated type
        enabled_kinds.insert("C".to_string()); // constant
        enabled_kinds.insert("v".to_string()); // variable/static
        enabled_kinds.insert("t".to_string()); // type alias
        enabled_kinds.insert("M".to_string()); // macro

        let mut config = Self {
            enabled_kinds,
            needs_traversal_cache: HashMap::new(),
        };
        config.rebuild_traversal_cache();
        config
    }

    /// Create a configuration from a kinds string (e.g., "nsf" or "n,s,f")
    pub fn from_kinds_string(kinds_str: &str) -> Self {
        let mut enabled_kinds = HashSet::new();

        // Handle both comma-separated and concatenated formats
        let kinds: Vec<&str> = if kinds_str.contains(',') {
            kinds_str.split(',').map(|s| s.trim()).collect()
        } else {
            // Split each character as a separate kind
            kinds_str
                .chars()
                .map(|c| match c {
                    'n' => "n",
                    's' => "s",
                    'g' => "g",
                    'u' => "u",
                    'i' => "i",
                    'c' => "c",
                    'f' => "f",
                    'P' => "P",
                    'm' => "m",
                    'e' => "e",
                    'T' => "T",
                    'C' => "C",
                    'v' => "v",
                    't' => "t",
                    'M' => "M",
                    _ => "", // Ignore unknown kinds
                })
                .filter(|s| !s.is_empty())
                .collect()
        };

        for kind in kinds {
            match kind {
                "n" | "module" => {
                    enabled_kinds.insert("n".to_string());
                }
                "s" | "struct" => {
                    enabled_kinds.insert("s".to_string());
                }
                "g" | "enum" => {
                    enabled_kinds.insert("g".to_string());
                }
                "u" | "union" => {
                    enabled_kinds.insert("u".to_string());
                }
                "i" | "trait" | "interface" => {
                    enabled_kinds.insert("i".to_string());
                }
                "c" | "impl" | "implementation" => {
                    enabled_kinds.insert("c".to_string());
                }
                "f" | "function" => {
                    enabled_kinds.insert("f".to_string());
                }
                "P" | "method" | "procedure" => {
                    enabled_kinds.insert("P".to_string());
                }
                "m" | "field" => {
                    enabled_kinds.insert("m".to_string());
                }
                "e" | "enumerator" | "variant" => {
                    enabled_kinds.insert("e".to_string());
                }
                "T" | "typedef" | "associated_type" => {
                    enabled_kinds.insert("T".to_string());
                }
                "C" | "constant" => {
                    enabled_kinds.insert("C".to_string());
                }
                "v" | "variable" | "static" => {
                    enabled_kinds.insert("v".to_string());
                }
                "t" | "type" | "alias" => {
                    enabled_kinds.insert("t".to_string());
                }
                "M" | "macro" => {
                    enabled_kinds.insert("M".to_string());
                }
                _ => {
                    eprintln!("Warning: Unknown Rust tag kind: {}", kind);
                }
            }
        }

        let mut config = Self {
            enabled_kinds,
            needs_traversal_cache: HashMap::new(),
        };
        config.rebuild_traversal_cache();
        config
    }

    /// Create a configuration with only specific kinds enabled
    pub fn with_kinds(kinds: &[&str]) -> Self {
        let enabled_kinds = kinds.iter().map(|k| k.to_string()).collect();
        let mut config = Self {
            enabled_kinds,
            needs_traversal_cache: HashMap::new(),
        };
        config.rebuild_traversal_cache();
        config
    }

    /// Enable a specific tag kind
    pub fn enable_kind(&mut self, kind: &str) -> &mut Self {
        self.enabled_kinds.insert(kind.to_string());
        self.rebuild_traversal_cache();
        self
    }

    /// Disable a specific tag kind
    pub fn disable_kind(&mut self, kind: &str) -> &mut Self {
        self.enabled_kinds.remove(kind);
        self.rebuild_traversal_cache();
        self
    }

    /// Check if a tag kind is enabled
    pub fn is_kind_enabled(&self, kind: &str) -> bool {
        self.enabled_kinds.contains(kind)
    }

    /// Get all enabled kinds
    pub fn enabled_kinds(&self) -> &HashSet<String> {
        &self.enabled_kinds
    }

    /// Check if we need to traverse into a specific node type for optimization
    pub fn needs_traversal(&self, node_kind: &str) -> bool {
        self.needs_traversal_cache
            .get(node_kind)
            .copied()
            .unwrap_or(true)
    }

    /// Rebuild the traversal optimization cache
    fn rebuild_traversal_cache(&mut self) {
        self.needs_traversal_cache.clear();

        // Define what child tags each node type can contain
        // Only traverse if we need the parent tag OR any potential child tags

        // Modules can contain everything
        self.needs_traversal_cache.insert(
            "mod_item".to_string(),
            self.is_kind_enabled("n") || self.needs_any_child_tags(),
        );

        // Structs can contain fields (tagged as 'm')
        self.needs_traversal_cache.insert(
            "struct_item".to_string(),
            self.is_kind_enabled("s") || self.is_kind_enabled("m"),
        );

        // Enums can contain variants (tagged as 'e')
        self.needs_traversal_cache.insert(
            "enum_item".to_string(),
            self.is_kind_enabled("g") || self.is_kind_enabled("e"),
        );

        // Unions are simple - no child tags typically
        self.needs_traversal_cache
            .insert("union_item".to_string(), self.is_kind_enabled("u"));

        // Traits can contain methods ('m'), associated types ('T'), constants ('C')
        self.needs_traversal_cache.insert(
            "trait_item".to_string(),
            self.is_kind_enabled("i")
                || self.is_kind_enabled("m")
                || self.is_kind_enabled("T")
                || self.is_kind_enabled("C"),
        );

        // Impl blocks can contain methods ('P'), associated types ('T'), constants ('C')
        self.needs_traversal_cache.insert(
            "impl_item".to_string(),
            self.is_kind_enabled("c")
                || self.is_kind_enabled("P")
                || self.is_kind_enabled("T")
                || self.is_kind_enabled("C"),
        );

        // Functions are leaf nodes - no child tags
        self.needs_traversal_cache.insert(
            "function_item".to_string(),
            self.is_kind_enabled("f") || self.is_kind_enabled("P"),
        );

        self.needs_traversal_cache.insert(
            "function_signature_item".to_string(),
            self.is_kind_enabled("m"),
        );

        // Other leaf nodes
        self.needs_traversal_cache
            .insert("associated_type".to_string(), self.is_kind_enabled("T"));
        self.needs_traversal_cache
            .insert("const_item".to_string(), self.is_kind_enabled("C"));
        self.needs_traversal_cache
            .insert("static_item".to_string(), self.is_kind_enabled("v"));
        self.needs_traversal_cache
            .insert("type_item".to_string(), self.is_kind_enabled("t"));
        self.needs_traversal_cache
            .insert("macro_definition".to_string(), self.is_kind_enabled("M"));
    }

    /// Helper to check if we need any child tags (for modules)
    fn needs_any_child_tags(&self) -> bool {
        // If any tag type is enabled, modules might need traversal
        !self.enabled_kinds.is_empty()
    }
}

impl Default for TagKindConfig {
    fn default() -> Self {
        Self::new()
    }
}

// Stores context during traversal
struct Context<'a> {
    source_code: &'a str,
    lines: Vec<Vec<u8>>,
    file_name: &'a str,
    tags: &'a mut Vec<tag::Tag>,
    tag_config: &'a TagKindConfig,
    user_config: &'a crate::config::Config,
    // Use a stack to keep track of nested scopes (module, struct, trait, etc.)
    scope_stack: Vec<(ScopeType, String)>,
}

impl<'a> Context<'a> {
    // Helper to get the text content of a node
    fn node_text(&self, node: &Node) -> &'a str {
        node.utf8_text(self.source_code.as_bytes())
            .unwrap_or_else(|_| {
                eprintln!(
                    "Warning: Failed to get UTF-8 text for node at range {:?}-{:?}",
                    node.start_position(),
                    node.end_position()
                );
                "" // Return empty string on error
            })
    }

    // Build extension fields based on the current scope stack
    fn create_extension_fields(&self) -> HashMap<String, String> {
        let mut fields = HashMap::new();
        let mut module_path = Vec::new();

        for (scope_type, name) in &self.scope_stack {
            match scope_type {
                ScopeType::Module => module_path.push(name.clone()),
                ScopeType::Struct => {
                    fields.insert(String::from("struct"), name.clone());
                }
                ScopeType::Enum => {
                    fields.insert(String::from("enum"), name.clone());
                }
                ScopeType::Union => {
                    fields.insert(String::from("union"), name.clone());
                }
                ScopeType::Trait => {
                    fields.insert(String::from("interface"), name.clone());
                }
                ScopeType::Implementation => {
                    // For impls, store the type being implemented.
                    // If it's a trait impl, the trait name might already be present
                    // from a Trait scope, or we can add it explicitly here if needed.
                    // Let's store the impl type under 'implementation' or 'impl_for'.
                    // Example `impl MyTrait for MyType` might have trait:MyTrait, implementation:MyType
                    // Example `impl MyType` would have implementation:MyType
                    fields.insert(String::from("implementation"), name.clone());
                    // Could also store the trait specifically if parsed:
                    // if let Some(trait_name) = find_trait_in_impl_scope(...) {
                    //     fields.insert(String::from("trait"), trait_name);
                    // }
                }
            }
        }

        if !module_path.is_empty() {
            fields.insert(String::from("module"), module_path.join("::"));
        }

        fields
    }
}

impl Parser {
    pub fn generate_rust_tags(
        &mut self,
        code: &[u8], // Changed to slice for flexibility
        file_path_relative_to_tag_file: &str,
    ) -> Option<Vec<tag::Tag>> {
        self.generate_rust_tags_with_config(
            code,
            file_path_relative_to_tag_file,
            &TagKindConfig::default(),
        )
    }

    pub fn generate_rust_tags_with_config(
        &mut self,
        code: &[u8], // Changed to slice for flexibility
        file_path_relative_to_tag_file: &str,
        tag_config: &TagKindConfig,
    ) -> Option<Vec<tag::Tag>> {
        self.generate_rust_tags_with_full_config(
            code,
            file_path_relative_to_tag_file,
            tag_config,
            &crate::config::Config::default(),
        )
    }

    pub fn generate_rust_tags_with_full_config(
        &mut self,
        code: &[u8], // Changed to slice for flexibility
        file_path_relative_to_tag_file: &str,
        tag_config: &TagKindConfig,
        user_config: &crate::config::Config,
    ) -> Option<Vec<tag::Tag>> {
        // Ensure the code is valid UTF-8
        let source_code = match std::str::from_utf8(code) {
            Ok(s) => s,
            Err(_) => {
                eprintln!(
                    "Warning: Input for {} is not valid UTF-8, skipping.",
                    file_path_relative_to_tag_file
                );
                return None;
            }
        };

        // Split the source code into lines for address generation
        let lines = split_by_newlines::split_by_newlines(code);

        self.ts_parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("Error loading Rust grammar");

        // Parse the source code using the parser from self
        let tree = self.ts_parser.parse(source_code, None)?; // Use ? for concise error handling

        let mut tags = Vec::new();
        {
            // New scope for cursor and context to avoid lifetime issues when returning tags
            let mut cursor = tree.walk();

            // Need to move the cursor to the root node's first child to start traversal
            if !cursor.goto_first_child() {
                return Some(tags); // Empty file or parse error
            }

            let mut context = Context {
                file_name: file_path_relative_to_tag_file,
                lines,
                source_code,
                tags: &mut tags,
                tag_config,
                user_config,
                scope_stack: vec![], // Initialize empty scope stack
            };

            walk(&mut cursor, &mut context);
        }

        Some(tags)
    }
}

// Depth-First Tree Traversal with Scope Management and Early Termination
fn walk(cursor: &mut TreeCursor, context: &mut Context) {
    loop {
        let node = cursor.node();
        let node_kind = node.kind();

        // Early termination: skip traversing this subtree if we don't need any tags from it
        if !context.tag_config.needs_traversal(node_kind) {
            // Skip to next sibling without processing this node or its children
            if !cursor.goto_next_sibling() {
                break; // No more siblings, return to parent level
            }
            continue;
        }

        // --- 1. Process the current node ---
        // Returns Option<(ScopeType, Name)> if a new scope is entered
        let scope_info = process_node(cursor, context);

        // --- 2. Manage Scope Stack ---
        let mut scope_pushed = false;
        if let Some((scope_type, scope_name)) = scope_info {
            // Avoid pushing empty names which can happen for anonymous blocks
            if !scope_name.is_empty() {
                context.scope_stack.push((scope_type, scope_name));
                scope_pushed = true;
            }
        }

        // --- 3. Recurse into children ---
        if cursor.goto_first_child() {
            walk(cursor, context);
        }

        // --- 4. Pop Scope if necessary (before moving to sibling) ---
        if scope_pushed {
            if context.scope_stack.pop().is_none() {
                // This should ideally not happen if push/pop logic is correct
                eprintln!(
                    "Warning: Popped from empty scope stack! Node: {:?}",
                    node_kind
                );
            }
        }

        // --- 5. Move to the next sibling ---
        if !cursor.goto_next_sibling() {
            cursor.goto_parent(); // Return cursor to current node after visiting children
            break; // No more siblings, return to parent level
        }
    }
}

// Dispatches node processing based on kind, returns scope info if node defines one
fn process_node(cursor: &mut TreeCursor, context: &mut Context) -> Option<(ScopeType, String)> {
    // Returns (ScopeType, Name) if scope changes
    let node = cursor.node();
    // println!("Node: {} @ {:?} Scope: {:?}", node.kind(), String::from_utf8_lossy(&context.lines[node.start_position().row]), context.scope_stack); // Debug print

    match node.kind() {
        "mod_item" => process_module(cursor, context),
        "struct_item" => process_struct(cursor, context),
        "enum_item" => process_enum(cursor, context),
        "union_item" => process_union(cursor, context),
        "trait_item" => process_trait(cursor, context),
        "impl_item" => process_impl(cursor, context),

        // Items often found inside traits or impls
        "function_item" => {
            // Determine if it's a method or a standalone function based on scope
            let kind_char = if let Some((top_scope, _)) = context.scope_stack.last() {
                match top_scope {
                    ScopeType::Implementation
                    | ScopeType::Struct
                    | ScopeType::Enum
                    | ScopeType::Union
                    | ScopeType::Trait => "P", // Method or default trait method
                    ScopeType::Module => "f", // Function within a module
                }
            } else {
                "f" // Top-level function
            };
            process_function(cursor, context, kind_char);
            None // Doesn't start a new named scope for context stack
        }
        "function_signature_item" => {
            // Function signatures within traits
            process_function(cursor, context, "m"); // Also tag as 'm'
            None
        }
        "associated_type" => {
            process_associated_type(cursor, context);
            None
        }
        "const_item" => {
            process_constant(cursor, context);
            None
        }

        // Other top-level items
        "static_item" => {
            process_variable(cursor, context);
            None
        }
        "type_item" => {
            // Type alias
            process_typedef(cursor, context);
            None
        }
        "macro_definition" => {
            process_macro(cursor, context);
            None // Macros don't typically form scopes in the way structs/traits
        }
        _ => None, // Ignore other node kinds for scope tracking / direct tagging
    }
}

// --- Tag Creation Helper ---

fn create_tag(
    name: String,
    kind_char: &str,
    node: Node, // Pass the node for position info
    context: &mut Context,
    // Allow passing extra fields specific to the item being tagged
    extra_fields: Option<HashMap<String, String>>,
) {
    if name.is_empty() || name == "_" {
        return; // Don't tag empty or placeholder names
    }

    // Check if this tag kind is enabled in the configuration
    if !context.tag_config.is_kind_enabled(kind_char) {
        return; // Skip creating this tag if the kind is disabled
    }

    let row = node.start_position().row;
    let address = address_string_from_line(row, context);
    let mut extension_fields = HashMap::new();

    // Add line number if enabled (n field)
    if context.user_config.fields_config.is_field_enabled("line") {
        extension_fields.insert(String::from("line"), (row + 1).to_string());
    }

    // Add kind in extension fields if enabled (k field)
    if context.user_config.fields_config.is_field_enabled("kind") {
        extension_fields.insert(String::from("kind"), kind_char.to_string());
    }

    // Add file field if enabled (f field) - indicates file-restricted scoping
    if context.user_config.fields_config.is_field_enabled("file") {
        extension_fields.insert(String::from("file"), "".to_string());
    }

    // Add end position if enabled (e field)
    if context.user_config.fields_config.is_field_enabled("end") {
        extension_fields.insert(
            String::from("end"),
            (node.end_position().row + 1).to_string(),
        );
    }

    // Add scope information if enabled (s field)
    if context.user_config.fields_config.is_field_enabled("scope")
        || context.user_config.extras_config.qualified
    {
        let scope_fields = context.create_extension_fields();
        extension_fields.extend(scope_fields);
    }

    // Merge extra fields if provided and their corresponding field types are enabled
    if let Some(extras) = extra_fields {
        for (key, value) in extras {
            match key.as_str() {
                "signature" => {
                    if context
                        .user_config
                        .fields_config
                        .is_field_enabled("signature")
                    {
                        extension_fields.insert(key, value);
                    }
                }
                "access" => {
                    if context.user_config.fields_config.is_field_enabled("access") {
                        extension_fields.insert(key, value);
                    }
                }
                "typeref" => {
                    if context
                        .user_config
                        .fields_config
                        .is_field_enabled("typeref")
                    {
                        extension_fields.insert(key, value);
                    }
                }
                "implementation" | "trait" | "struct" | "enum" | "union" => {
                    if context
                        .user_config
                        .fields_config
                        .is_field_enabled("implementation")
                        || context.user_config.fields_config.is_field_enabled("scope")
                        || context.user_config.extras_config.qualified
                    {
                        extension_fields.insert(key, value);
                    }
                }
                _ => {
                    // For other fields, include them if scope/qualified is enabled
                    if context.user_config.fields_config.is_field_enabled("scope")
                        || context.user_config.extras_config.qualified
                    {
                        extension_fields.insert(key, value);
                    }
                }
            }
        }
    }

    context.tags.push(tag::Tag {
        name,
        file_name: context.file_name.to_string(),
        address,
        kind: Some(String::from(kind_char)),
        extension_fields: if extension_fields.is_empty() {
            None
        } else {
            Some(extension_fields)
        },
    });
}
// --- Specific Node Processors (returning Scope Info) ---

fn process_module(cursor: &mut TreeCursor, context: &mut Context) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["identifier"]) {
        create_tag(name.clone(), "n", node, context, None); // 'n' for module
        Some((ScopeType::Module, name))
    } else {
        None
    }
}

fn process_struct(cursor: &mut TreeCursor, context: &mut Context) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["type_identifier"]) {
        create_tag(name.clone(), "s", node, context, None); // 's' for struct
        process_identifiers_list(cursor, context, &name, "m");
        cursor.goto_parent();
        Some((ScopeType::Struct, name))
    } else {
        None
    }
}

fn process_union(cursor: &mut TreeCursor, context: &mut Context) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["type_identifier"]) {
        create_tag(name.clone(), "u", node, context, None); // 'u' for union
        Some((ScopeType::Union, name))
    } else {
        None
    }
}

fn process_enum(cursor: &mut TreeCursor, context: &mut Context) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let enum_name = get_node_name(cursor, context, &["type_identifier"]);

    match &enum_name {
        None => None,
        Some(name) => {
            create_tag(name.clone(), "g", node, context, None); // 'g' for enum
            process_identifiers_list(cursor, context, name, "e");

            cursor.goto_parent(); // Back to enum_item node
                                  // --- End Process Enum Variants ---

            Some((ScopeType::Enum, name.clone()))
        }
    }
}

fn process_identifiers_list(
    cursor: &mut TreeCursor,
    context: &mut Context,
    name: &str,
    tag_kind: &str,
) {
    if !cursor.goto_first_child() {
        return;
    }

    let variant_type = if tag_kind == "e" { "enum" } else { "struct" };

    // --- Process Enum Variants ---
    loop {
        // Look for the list containing variants
        if cursor.node().kind() == "field_declaration_list"
            || cursor.node().kind() == "enum_variant_list"
        {
            if !cursor.goto_first_child() {
                return;
            }

            // Process each variant node
            loop {
                let kind = cursor.node().kind();
                if kind == "enum_variant" || kind == "field_declaration" {
                    let variant_node = cursor.node();
                    if let Some(variant_name) =
                        get_node_name(cursor, context, &["identifier", "field_identifier"])
                    {
                        // Add enum/struct name context specifically for the variant tag
                        let mut variant_fields = HashMap::new();
                        variant_fields.insert(variant_type.to_string(), name.to_owned());
                        create_tag(
                            variant_name,
                            tag_kind,
                            variant_node,
                            context,
                            Some(variant_fields),
                        );
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent(); // Back to field_declaration_list/enum_variant_list
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn process_trait(cursor: &mut TreeCursor, context: &mut Context) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["type_identifier"]) {
        create_tag(name.clone(), "i", node, context, None); // 'i' for trait
        Some((ScopeType::Trait, name))
    } else {
        None
    }
}

// Process 'impl_item' -> impl Foo { ... } or impl Bar for Foo { ... }
fn process_impl(cursor: &mut TreeCursor, context: &mut Context) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let (trait_name, type_name) = find_impl_names(cursor, context)?;

    let mut extra_fields = HashMap::new();
    let tag_name = type_name?.clone();
    let kind_char = "c";

    if let Some(tr_name) = &trait_name {
        extra_fields.insert("trait".to_string(), tr_name.clone());
    }

    create_tag(
        tag_name.clone(),
        kind_char,
        node,
        context,
        Some(extra_fields),
    );
    Some((ScopeType::Implementation, tag_name))
}

fn find_impl_names(
    cursor: &mut TreeCursor,
    context: &Context,
) -> Option<(Option<String>, Option<String>)> {
    let mut trait_name = None;
    let mut type_name = None;
    let mut found_for = false;

    if !cursor.goto_first_child() {
        return Some((trait_name, type_name));
    }

    loop {
        let child_node = cursor.node();
        match child_node.kind() {
            "type_identifier" | "scoped_type_identifier" | "generic_type" => {
                let name = context.node_text(&child_node).to_string();
                if found_for {
                    if type_name.is_none() {
                        type_name = Some(name);
                    }
                } else if trait_name.is_none() {
                    trait_name = Some(name);
                } else if type_name.is_none() {
                    type_name = Some(name);
                }
            }
            "for" => found_for = true,
            "declaration_list" | "{" => break,
            _ => {}
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent();

    if !found_for {
        type_name = trait_name.take();
    }

    Some((trait_name, type_name))
}
// Processes 'function_item' and 'function_signature_item'
fn process_function(
    cursor: &mut TreeCursor,
    context: &mut Context,
    kind_char: &str, // e.g., "f"
) {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["identifier"]) {
        let mut extra_fields = HashMap::new();

        // Only get the signature string if signature field is enabled
        if context
            .user_config
            .fields_config
            .is_field_enabled("signature")
        {
            if let Some(signature_str) = get_function_signature_string(node, cursor, context) {
                extra_fields.insert(String::from("signature"), signature_str);
            }
        }

        create_tag(
            name,
            kind_char,
            node,
            context,
            if extra_fields.is_empty() {
                None
            } else {
                Some(extra_fields)
            },
        );
    }
}

// Processes 'associated_type' -> type Item;
fn process_associated_type(cursor: &mut TreeCursor, context: &mut Context) {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["type_identifier"]) {
        // Using 'T' like type alias, context provides trait scope
        create_tag(name, "T", node, context, None);
    }
}

// Processes 'const_item'
fn process_constant(cursor: &mut TreeCursor, context: &mut Context) {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["identifier"]) {
        create_tag(name, "C", node, context, None); // 'c' for constant
    }
}

// Processes 'static_item'
fn process_variable(cursor: &mut TreeCursor, context: &mut Context) {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["identifier"]) {
        create_tag(name, "v", node, context, None); // 'v' for variable (ctags uses this for static)
    }
}

// Processes 'type_item' -> type MyType = ...;
fn process_typedef(cursor: &mut TreeCursor, context: &mut Context) {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["type_identifier"]) {
        create_tag(name, "t", node, context, None); // 'T' for type alias
    }
}

// Processes 'macro_definition' -> macro_rules! my_macro { ... }
fn process_macro(cursor: &mut TreeCursor, context: &mut Context) {
    let node = cursor.node();
    if let Some(name) = get_node_name(cursor, context, &["identifier", "metavariable"]) {
        let clean_name = name.strip_suffix('!').unwrap_or(&name).to_string();
        create_tag(clean_name, "M", node, context, None); // 'M' for macro definition
    }
}

// --- Helper Functions ---

// Finds the first child node matching any of the specified kinds and returns its text content.
// IMPORTANT: Temporarily modifies the cursor but restores it.
fn get_node_name(
    cursor: &mut TreeCursor, // Needs to be mutable to move
    context: &Context,
    kinds: &[&str],
) -> Option<String> {
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        let current_node = cursor.node();
        if kinds.contains(&current_node.kind()) {
            cursor.goto_parent(); // Restore cursor
            return Some(String::from(context.node_text(&current_node))); // Found the first match
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent(); // Restore cursor

    None
}

// Generates the ctags address string
fn address_string_from_line(row: usize, context: &Context) -> String {
    if row >= context.lines.len() {
        return format!("/^{}$/;", row + 1);
    }
    let line_bytes = &context.lines[row];
    let escaped = String::from_utf8_lossy(line_bytes)
        .replace('\\', "\\\\")
        .replace('/', "\\/")
        .replace('^', "\\^")
        .replace('$', "\\$");
    format!("/^{}$/;\"", escaped)
}

// oo

// Constructs the signature string for a function/method node.
// Example: "(param1: Type1, param2: Type2) -> ReturnType"
fn get_function_signature_string(
    func_node: Node,
    cursor: &mut TreeCursor,
    context: &Context,
) -> Option<String> {
    // The `parameters` node in tree-sitter-rust typically includes the parentheses.
    // Its text would be like "(param1: Type1, param2: Type2)" or "()".
    let params_text = match get_node_name(cursor, context, &["parameters"]) {
        Some(pt) => pt,
        None => return None, // Parameters are essential for a meaningful signature.
    };

    // For Return Type: "return_type" is a FIELD NAME on the function_item node.
    // The actual child node will have a KIND corresponding to the specific type (e.g., type_identifier).
    // We fetch the child by its field name, then get its text.
    let return_type_text_opt = func_node
        .child_by_field_name("return_type")
        .and_then(|rt_node| {
            let text = context.node_text(&rt_node).to_string();
            if text.is_empty() {
                // If the node's text is empty, treat as None
                None
            } else {
                Some(text)
            }
        });

    let raw_signature_str = if let Some(rt_text) = return_type_text_opt {
        // Only add "-> ReturnType" if rt_text is non-empty.
        if !rt_text.is_empty() {
            // Some(format!("{} -> {}", params_text, rt_text))
            format!("{} -> {}", params_text, rt_text)
        } else {
            params_text // Return type node exists but its text is empty.
        }
    } else {
        params_text // No return type node.
    };

    // Replace newlines and normalize whitespace to single spaces
    let cleaned_signature = raw_signature_str
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ");

    Some(cleaned_signature)
}
