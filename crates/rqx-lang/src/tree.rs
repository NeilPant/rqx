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

/// Every named descendant in source order, with the field its parent holds it in. The
/// cursor knows each field as it passes, where [`field_name`] has to find the parent
/// again for every node.
pub fn descendants_with_fields<'tree>(
    node: Node<'tree>,
) -> Vec<(Node<'tree>, Option<&'static str>)> {
    let mut found = Vec::new();
    let mut cursor = node.walk();
    let mut recursing = true;
    loop {
        if recursing {
            let current = cursor.node();
            if current.is_named() {
                // The walk starts at `node`, so the cursor cannot see its field.
                let field = if current == node {
                    field_name(node)
                } else {
                    cursor.field_name()
                };
                found.push((current, field));
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

/// What each comment under `root` is written above, by the comment's node id: the next
/// named sibling that is not a comment, or for a comment last in a block, what owns the
/// block. Worked out in one walk, because `Node::next_named_sibling` and `Node::parent`
/// each search down from the root of the tree and a file can hold thousands of comments.
pub fn comment_leads<'tree>(
    root: Node<'tree>,
    is_comment: impl Fn(Node<'tree>) -> bool,
    is_block: impl Fn(Node<'tree>) -> bool,
) -> std::collections::HashMap<usize, Node<'tree>> {
    let mut leads = std::collections::HashMap::new();
    let mut cursor = root.walk();
    let mut recursing = true;
    loop {
        if recursing {
            let current = cursor.node();
            if current.is_named() && is_comment(current) {
                let mut ahead = cursor.clone();
                let mut lead = None;
                while ahead.goto_next_sibling() {
                    let next = ahead.node();
                    if next.is_named() && !is_comment(next) {
                        lead = Some(next);
                        break;
                    }
                }
                if lead.is_none() {
                    let mut up = cursor.clone();
                    if up.goto_parent() && is_block(up.node()) && up.goto_parent() {
                        lead = Some(up.node());
                    }
                }
                if let Some(lead) = lead {
                    leads.insert(current.id(), lead);
                }
            }
            if cursor.goto_first_child() {
                continue;
            }
        }
        if cursor.goto_next_sibling() {
            recursing = true;
            continue;
        }
        if !cursor.goto_parent() || cursor.node() == root {
            return leads;
        }
        recursing = false;
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

#[cfg(test)]
mod tests {
    use rqx_core::Language;

    use super::*;
    use crate::cost::testing::tree_of;

    /// Comments above declarations, between statements, last in a block and trailing
    /// a file, in every language, with the walk checked against the node methods.
    const SAMPLES: [(Language, &str); 5] = [
        (
            Language::TypeScript,
            "// top\n/** doc */\nexport function f(a: number[]) {\n  // first\n  let x = 1;\n  // second\n  // third\n  for (const y of a) { x += y; /* last */ }\n  return x;\n  // trailing\n}\nclass C {\n  // field\n  n = 1;\n  /** method */\n  m() {}\n}\n// end\n",
        ),
        (
            Language::Python,
            "# top\ndef f(a):\n    \"\"\"doc\"\"\"\n    # first\n    x = 1\n    for y in a:\n        x += y\n        # last\n    return x\n# end\n",
        ),
        (
            Language::Go,
            "package p\n\n// F does it.\nfunc F(a []int) int {\n\t// first\n\tx := 0\n\tfor _, y := range a {\n\t\tx += y // inline\n\t}\n\t// trailing\n\treturn x\n}\n",
        ),
        (
            Language::Java,
            "class C {\n  // field\n  int n;\n  /** doc */\n  int f(int[] a) {\n    int x = 0;\n    // loop\n    for (int y : a) { x += y; }\n    return x; // inline\n    // trailing\n  }\n}\n",
        ),
        (
            Language::Cpp,
            "// top\n/// doc\nint f(const std::vector<int>& a) {\n  int x = 0; // inline\n  // loop\n  for (int y : a) { x += y; }\n  return x;\n  // trailing\n}\n",
        ),
    ];

    #[test]
    fn the_field_walk_agrees_with_asking_each_node() {
        for (language, source) in SAMPLES {
            let tree = tree_of(language, source);
            let root = tree.root_node();
            for scope in descendants(root) {
                let walked = descendants_with_fields(scope);
                let asked: Vec<_> = descendants(scope)
                    .into_iter()
                    .map(|node| (node, field_name(node)))
                    .collect();
                assert_eq!(walked, asked, "{language:?}: {}", kind_of(scope));
            }
        }
    }

    /// The node methods, which search from the root each time: what the walk replaces.
    fn lead_by_asking<'tree>(
        profile: &'static crate::profile::LanguageProfile,
        comment: Node<'tree>,
    ) -> Option<Node<'tree>> {
        let mut next = comment.next_named_sibling();
        while let Some(node) = next {
            if profile.is_comment(node) {
                next = node.next_named_sibling();
                continue;
            }
            return Some(node);
        }
        comment
            .parent()
            .filter(|parent| profile.is_block(*parent))
            .and_then(|block| block.parent())
    }

    #[test]
    fn comment_leads_agree_with_asking_each_comment() {
        for (language, source) in SAMPLES {
            let profile = crate::support(language).profile;
            let tree = tree_of(language, source);
            let root = tree.root_node();
            let leads = comment_leads(
                root,
                |node| profile.is_comment(node),
                |node| profile.is_block(node),
            );
            let comments: Vec<_> = descendants(root)
                .into_iter()
                .filter(|node| profile.is_comment(*node))
                .collect();
            assert!(
                comments.len() >= 4,
                "{language:?} has too few comments to test"
            );
            for comment in comments {
                assert_eq!(
                    leads.get(&comment.id()).copied(),
                    lead_by_asking(profile, comment),
                    "{language:?}: {}",
                    text_of(comment, source)
                );
            }
        }
    }
}
