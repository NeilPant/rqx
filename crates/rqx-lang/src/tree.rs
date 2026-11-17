use rqx_core::span::Span;
use tree_sitter::Node;

/// A node's kind, read from a table rather than from the grammar.
///
/// `Node::kind` is `CStr::from_ptr(ts_node_type(..)).to_str().unwrap()` — a foreign
/// call, a `strlen` and a UTF-8 validation, every time, for a string the grammar
/// already owns and never changes. A profile put eleven percent of a scan's
/// processor time in those three. The table is built once per grammar and indexed
/// by the node's own kind id, which is a field read.
///
/// The last table is kept beside the last grammar because a file is one grammar
/// from its first node to its last: the pointer check misses once per file and hits
/// on every node after it.
/// Takes the node by value or by reference: half the call sites hold one and half
/// hold the other, and `Node` is `Copy` either way.
pub fn kind_of<'tree>(node: impl std::borrow::Borrow<Node<'tree>>) -> &'static str {
    let node = *node.borrow();
    thread_local! {
        static LAST: std::cell::RefCell<Option<(tree_sitter::Language, &'static [&'static str])>> =
            const { std::cell::RefCell::new(None) };
    }
    static TABLES: std::sync::LazyLock<
        std::sync::Mutex<std::collections::HashMap<tree_sitter::Language, &'static [&'static str]>>,
    > = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

    let id = node.kind_id() as usize;
    let grammar = tree_sitter::Language::clone(&node.language());
    let table = LAST.with_borrow_mut(|last| {
        if let Some((held, table)) = last
            && *held == grammar
        {
            return *table;
        }
        let table = *TABLES
            .lock()
            .expect("the kind tables")
            .entry(tree_sitter::Language::clone(&grammar))
            .or_insert_with(|| {
                let names: Vec<&'static str> = (0..grammar.node_kind_count() as u16)
                    .map(|id| grammar.node_kind_for_id(id).unwrap_or_default())
                    .collect();
                // Leaked deliberately: one table per grammar for the life of the
                // process, and every grammar is itself a `&'static`.
                &*Box::leak(names.into_boxed_slice())
            });
        *last = Some((tree_sitter::Language::clone(&grammar), table));
        table
    });
    // A grammar can report a kind id past its own table for an error or a missing
    // node, and the grammar's own answer is right where the table has none.
    table.get(id).copied().unwrap_or_else(|| node.kind())
}

pub fn span_of(node: Node<'_>) -> Span {
    Span::new(node.start_byte() as u32, node.end_byte() as u32)
}

/// Sliced rather than read through `Node::utf8_text`, which runs `str::from_utf8`
/// over the bytes on every call. The source was validated once when it was read,
/// and this is the hottest accessor in the analyser: the validation alone was nine
/// percent of a scan's processor time.
pub fn text_of<'src>(node: Node<'_>, source: &'src str) -> &'src str {
    source.get(node.byte_range()).unwrap_or_default()
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
pub fn declares_through_list_under<'tree>(
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
    is_binding_position_under(
        parent,
        node,
        parameter_kinds,
        borrowed,
        declaring_assignments,
        lists,
    )
}

/// The same answer for a caller that already holds the parent, like `field_name_under`
/// beside `field_name` and for the same reason.
pub fn is_binding_position_under<'tree>(
    parent: Node<'tree>,
    node: Node<'tree>,
    parameter_kinds: &[&str],
    borrowed: &[&str],
    declaring_assignments: &[&str],
    lists: &[&str],
) -> bool {
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

/// Every named descendant in source order, without entering what a barrier owns.
/// The barrier is yielded and its contents are not: a callback is a node the caller
/// can see, but what it holds belongs to whatever the callback stands in for rather
/// than to the code around it.
///
/// This is the shape that replaced asking each descendant "is there a function
/// between me and here", which climbed with `Node::parent` — and `parent` walks down
/// from the root of the tree, so the question cost more than the walk that raised it.
pub fn descendants_outside<'tree>(
    node: Node<'tree>,
    barrier: impl Fn(Node<'tree>) -> bool,
) -> Vec<Node<'tree>> {
    let mut found = Vec::new();
    let mut cursor = node.walk();
    let mut recursing = true;
    loop {
        if recursing {
            let here = cursor.node();
            if here.is_named() {
                found.push(here);
            }
            if (here == node || !barrier(here)) && cursor.goto_first_child() {
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

/// Every named node under a file's root, listed once. The checks that each want "all
/// the comments" or "all the functions" filter this one list instead of each walking
/// the tree and allocating a list of its own.
pub struct Nodes<'tree> {
    root: Node<'tree>,
    named: Vec<Node<'tree>>,
}

impl<'tree> Nodes<'tree> {
    pub fn of(root: Node<'tree>) -> Self {
        Self {
            root,
            named: descendants(root),
        }
    }

    pub fn root(&self) -> Node<'tree> {
        self.root
    }

    pub fn iter(&self) -> impl Iterator<Item = Node<'tree>> + '_ {
        self.named.iter().copied()
    }

    /// The named nodes under `node`, which is the shared list only when `node` is the
    /// root: a nested node has fewer descendants than the file.
    pub fn under(&self, node: Node<'tree>) -> std::borrow::Cow<'_, [Node<'tree>]> {
        if node.id() == self.root.id() {
            std::borrow::Cow::Borrowed(&self.named)
        } else {
            std::borrow::Cow::Owned(descendants(node))
        }
    }
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
