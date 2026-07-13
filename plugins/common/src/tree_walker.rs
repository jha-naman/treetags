use tree_sitter::TreeCursor;

/// Implement this on your walker struct to use [`walk_tree`].
///
/// `process_node` is called for every node in pre-order. If the node opens a
/// scope, push it onto your internal stack and return `true` — the walker will
/// call `pop_scope` when backtracking past that node. Return `false` if no
/// scope was pushed.
pub trait WalkContext {
    fn process_node(&mut self, cursor: &TreeCursor) -> bool;
    fn pop_scope(&mut self);
}

/// Pre-order depth-first tree walk with integrated scope-stack management.
///
/// Calls `context.process_node` for every node and `context.pop_scope` on
/// backtrack for each node where `process_node` returned `true`.
pub fn walk_tree<C: WalkContext>(cursor: &mut TreeCursor, context: &mut C) {
    let mut scope_pushed_stack: Vec<bool> = Vec::new();

    loop {
        let pushed = context.process_node(cursor);
        scope_pushed_stack.push(pushed);

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
