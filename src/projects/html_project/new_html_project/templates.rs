//! Scaffold templates for `bean new html`.
//!
//! WHAT: Will own the generated content for `#config.bst`, `src/#page.bst`, manifests, and `.gitignore`.
//! WHY: Centralises template strings so they are not scattered through write logic.

/// Escape a string for use in a Beanstalk `#name = "..."` config literal.
///
/// Minimum escaping: backslash and double-quote.
// Phase 5 will use this when generating `#config.bst` content.
#[allow(dead_code)]
pub fn escape_beanstalk_string_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::escape_beanstalk_string_literal;

    #[test]
    fn escapes_backslash_and_quote() {
        assert_eq!(escape_beanstalk_string_literal(r#"a"b\c"#), r#"a\"b\\c"#);
    }

    #[test]
    fn leaves_other_characters_intact() {
        assert_eq!(
            escape_beanstalk_string_literal("hello world"),
            "hello world"
        );
    }

    #[test]
    fn escapes_multiple_quotes() {
        assert_eq!(
            escape_beanstalk_string_literal(r#""say" "hello""#),
            r#"\"say\" \"hello\""#
        );
    }
}
