use rqx_core::Language;
use tree_sitter::Node;

use crate::vocabulary::TestVocabulary;

/// Everything about a language that is a list of node kinds. What a table cannot
/// express — what counts as a declaration, what "public" means, how a name becomes
/// a path — lives behind `crate::support::LanguageSupport` instead.
pub struct LanguageProfile {
    pub language: Language,
    pub grammar: fn() -> tree_sitter::Language,
    /// Whether a specifier that resolved to no project file names something the
    /// project could replace, as opposed to the language's own standard library.
    /// Nothing is served by advising a team to wrap `java.util.List`.
    pub is_vendor: fn(&str) -> bool,
    /// Every member a class must declare to be iterable in this language. Empty where
    /// the language has no iteration protocol to implement.
    pub iteration_members: &'static [&'static str],
    /// Markers that leave a member for a subclass to fill in, where the language has
    /// one. Python's abstract methods carry a body, so an empty body cannot decide it.
    pub abstract_markers: &'static [&'static str],

    /// Nodes that own a parameter list and a body.
    pub function_kinds: &'static [&'static str],
    /// Nodes that own a set of members.
    pub class_kinds: &'static [&'static str],
    /// Members counted as behaviour.
    pub method_kinds: &'static [&'static str],
    /// Members counted as state.
    pub field_kinds: &'static [&'static str],
    /// The node holding a function's parameters, and the parameters inside it.
    pub parameter_list_kinds: &'static [&'static str],
    /// Wrappers that hold a declaration's name and parameters instead of the
    /// declaration holding them itself.
    pub declarator_wrapper_kinds: &'static [&'static str],
    pub parameter_kinds: &'static [&'static str],
    /// A name being bound to a value. The name reaches whatever function the value
    /// turns out to be, so an anonymous function assigned to one is not anonymous to
    /// a reader.
    pub declarator_kinds: &'static [&'static str],

    /// One unit of work in a body. Statement count decides `long-function`.
    pub statement_kinds: &'static [&'static str],
    /// A run of statements. Clone candidates are taken at these boundaries.
    pub block_kinds: &'static [&'static str],
    /// Parents whose block is part of their own syntax rather than a statement
    /// standing where a statement could stand. `try { }` has no choice about its
    /// braces, so counting them would make every `try` cost a statement an author
    /// never wrote. Function and class bodies are excluded without being listed.
    pub structural_block_parents: &'static [&'static str],

    /// A decision that also nests what follows it.
    pub branch_kinds: &'static [&'static str],
    /// A decision that does not nest: `else`, `case`, a jump to a label.
    pub flat_branch_kinds: &'static [&'static str],
    /// Handled apart from `branch_kinds` because `else if` is flattened rather than
    /// counted as a nested `if`, per the SonarSource specification.
    pub if_kind: &'static str,
    pub else_field: &'static str,
    pub catch_kinds: &'static [&'static str],

    pub comment_kinds: &'static [&'static str],
    pub number_kinds: &'static [&'static str],
    pub string_kinds: &'static [&'static str],
    /// Every node kind that spells a plain name.
    pub identifier_kinds: &'static [&'static str],
    /// Names that can only ever be a use, never a declaration — a shorthand object
    /// key stands for the value it names.
    pub reference_only_kinds: &'static [&'static str],
    /// The name of a member being reached for: `.length`, `->size`, `::value`.
    pub property_kinds: &'static [&'static str],
    /// Reaching through one object to another. Depth decides `message-chain`.
    pub member_access_kinds: &'static [&'static str],
    /// The node kind carrying a binary operator, and the operators that make a
    /// decision a reader has to hold.
    pub binary_kinds: &'static [&'static str],
    pub logical_operators: &'static [&'static str],
    pub negation_operators: &'static [&'static str],
    pub equality_operators: &'static [&'static str],
    /// What this language writes for true and false, so a comparison against one can
    /// be told from a comparison against a value.
    pub boolean_literals: &'static [&'static str],
    /// Repetition, for the rules about a body that decides nothing.
    pub loop_kinds: &'static [&'static str],
    pub call_kinds: &'static [&'static str],
    /// The field naming what a call calls. Some grammars hold the whole callee in one
    /// field; others put the receiver and the method name in separate ones, in which
    /// case the call node is itself the thing to measure a chain from.
    pub callee_field: &'static str,
    /// Names written with a namespace separator, which spell one dotted name.
    pub qualified_name_kinds: &'static [&'static str],

    /// What the receiver is called inside a method body.
    pub self_names: &'static [&'static str],
    /// A parameter or field spelled with this prefix is the author saying they
    /// already know it is unused.
    pub ignored_prefixes: &'static [&'static str],

    /// Nodes that open a scope: a name declared inside one is invisible outside it.
    pub scope_kinds: &'static [&'static str],
    /// The fields under which an identifier is being declared rather than used.
    pub binding_fields: &'static [&'static str],
    /// Nodes that bind a name brought in from another file.
    pub import_binding_kinds: &'static [&'static str],
    /// Nodes that make what they contain visible outside the file.
    pub export_kinds: &'static [&'static str],
    /// Argument lists. A function written inside one has its arity fixed by whatever
    /// is being called, not by its author.
    pub argument_list_kinds: &'static [&'static str],
    /// Nodes saying a class has a supertype, whose signatures it cannot change.
    pub heritage_kinds: &'static [&'static str],
    /// A declared type on a binding: it, not the author, fixes the shape of any
    /// function stored there.
    pub type_annotation_kinds: &'static [&'static str],
    /// A key-value member of an object literal, which takes its shape from the
    /// object's declared type.
    pub property_definition_kinds: &'static [&'static str],
    /// Modifiers that turn a parameter into a field declaration.
    pub field_declaring_modifiers: &'static [&'static str],

    /// Statements that end a run: nothing written after one in the same block is
    /// reached. The second word is how the finding reads.
    pub exit_kinds: &'static [(&'static str, &'static str)],
    /// Declarations that stay reachable after an exit because the language hoists
    /// them.
    pub hoisted_kinds: &'static [&'static str],
    /// Nodes that write to something.
    pub assignment_kinds: &'static [&'static str],
    /// Wrappers that add no meaning and must be seen through when counting operands.
    pub transparent_kinds: &'static [&'static str],
    /// The field holding a branch's condition.
    pub condition_field: &'static str,
    /// The field naming what is being reached for through an object: `.length` is a
    /// `property` in one grammar and an `attribute` in another.
    pub property_field: &'static str,
    /// The field naming what is being reached through: `a.b` puts `a` in an `object`
    /// field in one grammar and an `operand` field in another.
    pub object_field: &'static str,
    /// Wrappers that hold a list of expressions where one was expected, which have to
    /// be seen through to find the name a statement actually binds.
    pub expression_list_kinds: &'static [&'static str],
    /// Operators whose presence in a comment line means the line is code.
    pub code_operators: &'static [&'static str],
    /// Whether `else` and `else if` hang off the `if` as siblings in one list, rather
    /// than nesting as a chain. Python writes `elif` as a sibling clause; C-family
    /// languages nest a second `if` inside the first's alternative.
    pub else_is_sibling_list: bool,
    /// The clauses that follow an `if` when they are written as siblings.
    pub else_clause_kinds: &'static [&'static str],
    /// Class bodies with no field syntax say it with an assignment instead. Non-empty
    /// here also means an assignment to the receiver inside a method declares a field.
    pub assigned_field_kinds: &'static [&'static str],
    /// Assignments that also declare, for languages with no `let`: the first write to
    /// a name is its declaration. `+=` is never one of these — it reads the name
    /// before it writes it, so it cannot be where the name begins.
    pub local_declaring_kinds: &'static [&'static str],
    /// The field naming what a `switch` branches on.
    pub switch_subject_field: &'static str,
    /// Wrappers that carry annotations above a definition, and the annotations
    /// themselves. A modifier written there belongs to the definition inside.
    pub decorated_kinds: &'static [&'static str],
    pub decorator_kinds: &'static [&'static str],
    /// A declared type that gives the checker nothing, and the label to report.
    pub imprecise_type_kinds: &'static [(&'static str, &'static str)],
    /// Type names that mean "anything".
    pub imprecise_type_names: &'static [&'static str],
    /// Text that makes an otherwise imprecise construct precise, such as TypeScript's
    /// `as const`, which narrows rather than widens.
    pub imprecise_type_exceptions: &'static [&'static str],
    /// What this language calls its boolean, for the rule about flag arguments.
    pub boolean_type_names: &'static [&'static str],
    /// Directives that silence a checker, whether written in a comment or as an
    /// annotation on the thing being silenced.
    pub suppression_directives: &'static [&'static str],
    /// Annotation nodes, which some languages use in place of a comment directive.
    pub annotation_kinds: &'static [&'static str],
    /// How a line of this language ends, and how one begins: together they separate
    /// commented-out code from prose.
    pub code_line_endings: &'static [&'static str],
    pub code_markers: &'static [&'static str],
    /// Words that are syntax rather than the subject of a comment.
    pub declaration_keywords: &'static [&'static str],
    /// Statements that mean "do nothing", so a body made only of them is empty.
    pub empty_body_kinds: &'static [&'static str],
    /// Raising an error. A body that only does this is refusing what it inherited.
    pub throw_kinds: &'static [&'static str],
    /// What this language calls the member that runs on construction.
    pub constructor_names: &'static [&'static str],
    /// Nodes that gather a member's modifiers instead of listing them beside it.
    pub modifier_container_kinds: &'static [&'static str],
    /// Labels that set the visibility of everything after them, rather than of one
    /// member. C++ writes `private:` once and means it until the next label.
    pub access_section_kinds: &'static [&'static str],
    /// Whether a member with no visibility label is hidden. `class` in C++ starts
    /// private; `struct` and every other language here start public.
    pub default_members_are_private: bool,
    /// Wrappers that hold the name a declaration binds, where the declaration itself
    /// does not name it directly.
    pub name_holder_kinds: &'static [&'static str],
    /// Members that run on construction, where the language gives them their own node
    /// kind rather than a reserved name.
    pub constructor_kinds: &'static [&'static str],
    /// Prefixes that make a free function the constructor of the type it names, for
    /// languages that have no constructor of their own. Go's `NewEngine` is the only
    /// place an `Engine`'s fields are set up.
    pub constructor_name_prefixes: &'static [&'static str],
    /// A key in a composite literal names the field it sets, which is how a language
    /// without constructors initialises one.
    pub literal_field_kinds: &'static [&'static str],
    /// Markers, by node kind or by text, that hide a member from outside the class.
    pub private_markers: &'static [&'static str],
    /// Naming conventions that stand in for a keyword the language does not have.
    pub private_name_prefixes: &'static [&'static str],
    /// Markers that make a member belong to the type rather than to an instance.
    pub static_markers: &'static [&'static str],
    /// Markers for members that read as fields but are written as methods.
    pub accessor_markers: &'static [&'static str],
    /// A branch on one value, and the arms inside it.
    pub switch_kinds: &'static [&'static str],
    pub case_kinds: &'static [&'static str],
    /// The clause naming a single superclass, as opposed to interfaces implemented.
    pub extends_kinds: &'static [&'static str],
    /// A name given to a type, and the union and literal members it may be made of.
    pub type_alias_kinds: &'static [&'static str],
    /// The fields naming an alias and what it stands for.
    pub alias_name_field: &'static str,
    pub alias_value_field: &'static str,
    pub union_type_kinds: &'static [&'static str],
    pub literal_type_kinds: &'static [&'static str],
    /// Members declared without a body, where a property's name still carries a type.
    pub signature_kinds: &'static [&'static str],
    /// Prefixes that spell a type into a name, and what each one claims.
    pub hungarian_prefixes: &'static [(&'static str, &'static str)],
    /// Every kind that can spell a name being declared, including the ones a language
    /// reserves for types.
    pub binding_identifier_kinds: &'static [&'static str],
    /// Nodes that name something declared in another file rather than declaring it.
    pub borrowed_name_kinds: &'static [&'static str],
    pub test: TestVocabulary,
}

impl LanguageProfile {
    pub fn is_function(&self, node: Node<'_>) -> bool {
        self.function_kinds.contains(&node.kind())
    }

    pub fn is_class(&self, node: Node<'_>) -> bool {
        self.class_kinds.contains(&node.kind())
    }

    pub fn is_statement(&self, node: Node<'_>) -> bool {
        self.statement_kinds.contains(&node.kind())
    }

    pub fn is_block(&self, node: Node<'_>) -> bool {
        self.block_kinds.contains(&node.kind())
    }

    pub fn is_comment(&self, node: Node<'_>) -> bool {
        self.comment_kinds.contains(&node.kind())
    }

    pub fn is_identifier(&self, node: Node<'_>) -> bool {
        self.identifier_kinds.contains(&node.kind())
    }

    pub fn is_member_access(&self, node: Node<'_>) -> bool {
        self.member_access_kinds.contains(&node.kind())
    }

    pub fn is_call(&self, node: Node<'_>) -> bool {
        self.call_kinds.contains(&node.kind())
    }

    pub fn is_ignored_name(&self, name: &str) -> bool {
        self.ignored_prefixes
            .iter()
            .any(|prefix| name.starts_with(prefix))
    }
}

#[cfg(test)]
mod tests {
    /// A kind named in a profile that the grammar does not have never matches anything,
    /// which is a rule silently switched off rather than a loud failure.
    #[test]
    fn every_named_kind_exists_in_its_grammar() {
        for language in rqx_core::Language::ALL {
            let profile = crate::support(language).profile;
            let grammar = (profile.grammar)();
            let known: std::collections::HashSet<&str> = (0..grammar.node_kind_count())
                .filter_map(|id| grammar.node_kind_for_id(id as u16))
                .collect();
            for (label, kinds) in [
                ("function_kinds", profile.function_kinds),
                ("class_kinds", profile.class_kinds),
                ("statement_kinds", profile.statement_kinds),
                ("block_kinds", profile.block_kinds),
                ("comment_kinds", profile.comment_kinds),
                ("identifier_kinds", profile.identifier_kinds),
                ("member_access_kinds", profile.member_access_kinds),
                ("call_kinds", profile.call_kinds),
                ("loop_kinds", profile.loop_kinds),
            ] {
                for name in kinds {
                    assert!(
                        known.contains(name),
                        "{language:?} {label} names `{name}`, which its grammar does not have"
                    );
                }
            }
        }
    }
}
