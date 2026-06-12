use crate::compiler_frontend::keywords::{
    is_identifier_continue, is_keyword, is_valid_identifier, keyword_token_kind,
};
use crate::compiler_frontend::symbols::identifier_policy::keyword_shadow_match;
use crate::compiler_frontend::tokenizer::tokens::TokenKind;

#[test]
fn keyword_policy_maps_exact_tokenizer_spellings() {
    let exact_keywords = [
        ("import", TokenKind::Import),
        ("export", TokenKind::Export),
        ("this", TokenKind::This),
        ("This", TokenKind::TraitThis),
        ("true", TokenKind::BoolLiteral(true)),
        ("True", TokenKind::DatatypeTrue),
        ("none", TokenKind::NoneLiteral),
        ("None", TokenKind::DatatypeNone),
        ("to", TokenKind::ExclusiveRange),
        ("copy", TokenKind::Copy),
        ("cast", TokenKind::Cast),
    ];

    for (source, expected_kind) in exact_keywords {
        assert_eq!(keyword_token_kind(source), Some(expected_kind));
        assert!(is_keyword(source));
    }
}

#[test]
fn keyword_policy_keeps_case_sensitive_non_keywords_as_identifiers() {
    assert_eq!(keyword_token_kind("Import"), None);
    assert_eq!(keyword_token_kind("Copy"), None);

    assert!(is_valid_identifier("Import"));
    assert!(is_valid_identifier("_copy"));
}

#[test]
fn keyword_shadow_policy_shares_the_canonical_keyword_set() {
    assert_eq!(keyword_shadow_match("__LoOp"), Some("loop"));
    assert_eq!(keyword_shadow_match("_FALSE"), Some("false"));
    assert_eq!(keyword_shadow_match("_not_a_keyword"), None);

    assert_eq!(keyword_shadow_match("export"), Some("export"));
    assert_eq!(keyword_shadow_match("EXPORT"), Some("export"));
    assert_eq!(keyword_shadow_match("_export"), Some("export"));

    assert_eq!(keyword_shadow_match("cast"), Some("cast"));
    assert_eq!(keyword_shadow_match("CAST"), Some("cast"));
    assert_eq!(keyword_shadow_match("_cast"), Some("cast"));
}

#[test]
fn identifier_policy_matches_tokenizer_identifier_characters() {
    assert!(is_identifier_continue('a'));
    assert!(is_identifier_continue('9'));
    assert!(is_identifier_continue('_'));
    assert!(!is_identifier_continue('-'));

    assert!(is_valid_identifier("_valid_12"));
    assert!(!is_valid_identifier("12_invalid"));
    assert!(!is_valid_identifier("bad-name"));
}
