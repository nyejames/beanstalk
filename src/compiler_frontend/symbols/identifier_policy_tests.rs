use super::*;

#[test]
fn keyword_shadow_matching_ignores_case_and_leading_underscores() {
    assert_eq!(keyword_shadow_match("_true"), Some("true"));
    assert_eq!(keyword_shadow_match("FALSE"), Some("false"));
    assert_eq!(keyword_shadow_match("__LoOp"), Some("loop"));
    assert_eq!(keyword_shadow_match("block"), Some("block"));
    assert_eq!(keyword_shadow_match("Block"), Some("block"));
    assert_eq!(keyword_shadow_match("_BLOCK"), Some("block"));
    assert_eq!(keyword_shadow_match("checked"), Some("checked"));
    assert_eq!(keyword_shadow_match("Checked"), Some("checked"));
    assert_eq!(keyword_shadow_match("_async"), Some("async"));
}

#[test]
fn keyword_shadow_matching_rejects_non_keywords() {
    assert_eq!(keyword_shadow_match("value"), None);
    assert_eq!(keyword_shadow_match("_"), None);
    assert_eq!(keyword_shadow_match("___"), None);
    assert_eq!(keyword_shadow_match("error"), None);
    assert_eq!(keyword_shadow_match("_This"), None);
}

#[test]
fn type_and_value_style_helpers_follow_policy() {
    assert!(is_camel_case_type_name("User"));
    assert!(is_camel_case_type_name("Http2Client"));
    assert!(!is_camel_case_type_name("user"));
    assert!(!is_camel_case_type_name("User_Name"));

    assert!(is_lowercase_with_underscores_name("user_name"));
    assert!(is_lowercase_with_underscores_name("_user_name"));
    assert!(is_lowercase_with_underscores_name("value2"));
    assert!(!is_lowercase_with_underscores_name("VALUE"));
    assert!(!is_lowercase_with_underscores_name("__"));

    assert!(is_uppercase_constant_name("SITE_NAME"));
    assert!(is_uppercase_constant_name("HTTP2_PORT"));
    assert!(!is_uppercase_constant_name("Site_Name"));
    assert!(!is_uppercase_constant_name("___"));
}
