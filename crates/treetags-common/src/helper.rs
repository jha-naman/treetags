/// Control flow for child iteration
pub enum IterationControl {
    Continue,
    Break,
}

/// Iterate over the children of the cursor's current node
#[macro_export]
macro_rules! iterate_children {
    ($cursor:expr, |$node:ident| $body:block) => {
        if $cursor.goto_first_child() {
            loop {
                let $node = $cursor.node();
                let control = $body;
                match control {
                    $crate::helper::IterationControl::Break => break,
                    $crate::helper::IterationControl::Continue => {}
                }
                if !$cursor.goto_next_sibling() {
                    break;
                }
            }
            $cursor.goto_parent();
        }
    };
}
