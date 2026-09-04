use rqx_core::span::Span;
use tree_sitter::Node;

pub fn span_of(node: Node<'_>) -> Span {
    Span::new(node.start_byte() as u32, node.end_byte() as u32)
}

pub fn text_of<'src>(node: Node<'_>, source: &'src str) -> &'src str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

/// The field this node fills in its parent, if it fills one. Grammars use fields to
/// say which child is the name and which is the body, which is the only portable way
/// to tell a declaration from a use.
pub fn field_name(node: Node<'_>) -> Option<&'static str> {
    field_name_under(node.parent()?, node)
}

/// The same answer for a caller that already holds the parent. `Node::parent` walks
/// down from the root of the tree to find it, so the hot paths ask once and pass it
/// along rather than paying for it in each helper they call.
pub fn field_name_under<'tree>(parent: Node<'tree>, node: Node<'tree>) -> Option<&'static str> {
    let mut cursor = parent.walk();
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        if cursor.node() == node {
            return cursor.field_name();
        }
        if !cursor.goto_next_sibling() {
            return None;
        }
    }
}

/// The operator a node was written with. Most grammars put it in an `operator`
/// field; some spell it as a bare token among the children, which is the only place
/// left to look.
pub fn operator_of<'src>(node: Node<'_>, source: &'src str) -> Option<&'src str> {
    if let Some(operator) = node.child_by_field_name("operator") {
        return Some(text_of(operator, source));
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| !child.is_named() && !child.is_extra())
        .map(|child| text_of(child, source))
}

/// Whether this name is what an assignment declares, seeing through any list the
/// grammar wraps the left-hand side in.
pub fn declares_through_list(node: Node<'_>, declaring: &[&str], lists: &[&str]) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    declares_through_list_under(parent, node, declaring, lists)
}

fn declares_through_list_under<'tree>(
    parent: Node<'tree>,
    node: Node<'tree>,
    declaring: &[&str],
    lists: &[&str],
) -> bool {
    if declaring.contains(&parent.kind()) {
        return field_name_under(parent, node) == Some("left");
    }
    if !lists.contains(&parent.kind()) || field_name(parent) != Some("left") {
        return false;
    }
    parent
        .parent()
        .is_some_and(|owner| declaring.contains(&owner.kind()))
}

/// The operands of a binary node. Most grammars name them `left` and `right`; some
/// leave a comparison's operands unnamed, where position is the only thing left.
pub fn operands_of<'tree>(node: Node<'tree>) -> Option<(Node<'tree>, Node<'tree>)> {
    if let (Some(left), Some(right)) = (
        node.child_by_field_name("left"),
        node.child_by_field_name("right"),
    ) {
        return Some((left, right));
    }
    let mut cursor = node.walk();
    let named: Vec<Node<'tree>> = node.named_children(&mut cursor).collect();
    match named.as_slice() {
        [left, .., right] => Some((*left, *right)),
        _ => None,
    }
}

/// Whether this identifier is being declared rather than used. Grammars use fields
/// to say which child is the name, which is the only portable way to tell the two
/// apart without a symbol table.
pub fn is_binding_position(
    node: Node<'_>,
    parameter_kinds: &[&str],
    borrowed: &[&str],
    declaring_assignments: &[&str],
    lists: &[&str],
) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if declares_through_list_under(parent, node, declaring_assignments, lists) {
        return true;
    }
    // A re-export names something declared in another file: it fills a `name` field
    // without declaring anything.
    if borrowed.contains(&parent.kind()) {
        return false;
    }
    if matches!(
        field_name_under(parent, node),
        Some("name" | "pattern" | "declarator")
    ) {
        return true;
    }
    parameter_kinds.contains(&parent.kind())
}

/// Named descendants of one of the given kinds, without descending into the parts
/// this is not about. A function's parameter list can sit under a declarator rather
/// than beside the body, and a nested function's must not be mistaken for it.
pub fn find_within<'tree>(
    node: Node<'tree>,
    wanted: &[&str],
    barriers: &[&str],
) -> Vec<Node<'tree>> {
    let mut found = Vec::new();
    let mut queue = vec![node];
    while let Some(current) = queue.pop() {
        let mut cursor = current.walk();
        for child in current.named_children(&mut cursor) {
            if wanted.contains(&child.kind()) {
                found.push(child);
                continue;
            }
            if !barriers.contains(&child.kind()) {
                queue.push(child);
            }
        }
    }
    found.sort_by_key(|node| node.start_byte());
    found
}

/// Every named descendant, in source order.
pub fn descendants<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    let mut found = Vec::new();
    let mut cursor = node.walk();
    let mut recursing = true;
    loop {
        if recursing {
            if cursor.node().is_named() {
                found.push(cursor.node());
            }
            if cursor.goto_first_child() {
                continue;
            }
        }
        if cursor.goto_next_sibling() {
            recursing = true;
            continue;
        }
        if !cursor.goto_parent() || cursor.node() == node {
            return found;
        }
        recursing = false;
    }
}
