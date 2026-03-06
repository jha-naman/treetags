use tree_sitter::TreeCursor;

/// Trait for language-specific context behavior
pub trait LanguageContext {
    type ScopeType;

    fn push_scope(&mut self, scope_type: Self::ScopeType, name: String);
    fn pop_scope(&mut self) -> Option<(Self::ScopeType, String)>;
    fn process_node(&mut self, cursor: &mut TreeCursor) -> Option<(Self::ScopeType, String)>;
}

/// Generic tree walking function that can be used by any language implementation
/// that implements the LanguageContext trait
pub fn walk_generic<C: LanguageContext>(cursor: &mut TreeCursor, context: &mut C) {
    let mut scope_pushed_stack: Vec<bool> = Vec::new();

    // Pre-order tree walk with scope stack management
    loop {
        let scope_info = context.process_node(cursor);
        let mut scope_pushed = false;
        if let Some((scope_type, scope_name)) = scope_info {
            if !scope_name.is_empty() {
                context.push_scope(scope_type, scope_name);
                scope_pushed = true;
            }
        }
        scope_pushed_stack.push(scope_pushed);

        if cursor.goto_first_child() {
            continue;
        }

        loop {
            if scope_pushed_stack.pop().unwrap_or(false) {
                context.pop_scope();
            }

            if cursor.goto_next_sibling() {
                break;
            }

            if !cursor.goto_parent() {
                return;
            }
        }
    }
}
