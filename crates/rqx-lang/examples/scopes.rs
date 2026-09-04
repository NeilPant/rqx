//! Prints what the scope resolver found, which is what every `unused-*` rule reads.

fn main() {
    let mut args = std::env::args().skip(1);
    let which = args.next().expect("usage: scopes <language> <file>");
    let file = args.next().expect("usage: scopes <language> <file>");
    let language =
        rqx_core::Language::parse(&which).unwrap_or_else(|| panic!("unknown language {which}"));
    let profile = rqx_lang::support(language).profile;

    let source = std::fs::read_to_string(&file).expect("read");
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&(profile.grammar)()).expect("grammar");
    let tree = parser.parse(&source, None).expect("parse");
    let scopes = rqx_lang::scope::resolve(profile, &source, tree.root_node());

    for binding in &scopes.bindings {
        println!(
            "{:?} {} refs={} scope={}",
            binding.kind,
            binding.name,
            binding.references.len(),
            binding.scope
        );
    }
}
