use rqx_core::Language;

use crate::profile::LanguageProfile;
use crate::typescript::TYPESCRIPT_PROFILE;

/// Parsed with the TSX grammar whatever the extension: JSX is common in `.js`, and
/// the one thing TSX cannot read that TypeScript can — the `<T>value` cast — is not
/// JavaScript, so nothing is ambiguous.
///
/// Every table that names type syntax is emptied. A rule that reads one then has
/// nothing to match, which is the same fact the catalogue declares as an exemption.
pub static JAVASCRIPT: LanguageProfile = LanguageProfile {
    language: Language::JavaScript,
    grammar: || tree_sitter_typescript::LANGUAGE_TSX.into(),
    markup_grammar: None,
    abstract_markers: &[],
    imprecise_type_kinds: &[],
    imprecise_type_names: &[],
    boolean_type_names: &[],
    type_alias_kinds: &[],
    union_type_kinds: &[],
    literal_type_kinds: &[],
    ..TYPESCRIPT_PROFILE
};
