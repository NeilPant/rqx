//! Prints the parse tree Rqx sees, which is how a language profile's node kinds are
//! found in the first place.

fn main() {
    let mut args = std::env::args().skip(1);
    let which = args.next().expect("usage: dump <language> <file>");
    let file = args.next().expect("usage: dump <language> <file>");
    let language =
        rqx_core::Language::parse(&which).unwrap_or_else(|| panic!("unknown language {which}"));
    let grammar = rqx_lang::grammar(language);

    let source = std::fs::read_to_string(&file).expect("read");
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&grammar).expect("grammar");
    let tree = parser.parse(&source, None).expect("parse");

    let mut cursor = tree.walk();
    let mut depth = 0usize;
    loop {
        let node = cursor.node();
        if node.is_named() {
            let text = node.utf8_text(source.as_bytes()).unwrap_or("");
            let head: String = text.chars().take(48).collect();
            println!(
                "{:indent$}{} :: {}",
                "",
                node.kind(),
                head.replace('\n', "\\n"),
                indent = depth * 2
            );
        }
        if cursor.goto_first_child() {
            depth += 1;
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return;
            }
            depth -= 1;
        }
    }
}
