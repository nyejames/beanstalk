//! Shared identifier naming and reserved-keyword policy helpers.
//!
//! WHAT: centralizes naming-style warnings and keyword-shadow reservation checks used by
//! header parsing and AST binding creation.
//! WHY: identifier rules should not drift between frontend stages; one module keeps policy
//! and diagnostics consistent.

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey};
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::source_location::SourceLocation;

/// Canonical keyword set used by the tokenizer's `keyword_or_variable()` mapping.
///
/// NOTE: matching is case-insensitive and ignores any number of leading underscores.
const RESERVED_KEYWORD_SHADOWS: [&str; 29] = [
    "import", "if", "case", "return", "yield", "else", "as", "copy", "must",
    "loop", "in", "to", "upto", "by", "break", "continue", "is", "not", "and", "or",
    "true", "false", "none", "fn", "float", "int", "string", "bool", "char",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IdentifierNamingKind {
    TypeLike,
    ValueLike,
    TopLevelConstant,
}

/// Returns `name` without leading underscore characters.
pub(crate) fn strip_leading_underscores(name: &str) -> &str {
    name.trim_start_matches('_')
}

/// Returns the canonical keyword matched by this identifier shadow, if any.
///
/// Example shadows: `_true`, `FALSE`, `__LoOp`.
pub(crate) fn keyword_shadow_match(name: &str) -> Option<&'static str> {
    let stripped = strip_leading_underscores(name);
    if stripped.is_empty() {
        return None;
    }

    RESERVED_KEYWORD_SHADOWS
        .iter()
        .copied()
        .find(|keyword| stripped.eq_ignore_ascii_case(keyword))
}

pub(crate) fn is_keyword_shadow_identifier(name: &str) -> bool {
    keyword_shadow_match(name).is_some()
}

/// Returns true for CamelCase-style type identifiers: `^[A-Z][A-Za-z0-9]*$`.
pub(crate) fn is_camel_case_type_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    first.is_ascii_uppercase() && chars.all(|ch| ch.is_ascii_alphanumeric())
}

/// Returns true for lowercase_with_underscores identifiers.
///
/// Rule: lowercase letters/digits/underscores only, and at least one lowercase letter.
pub(crate) fn is_lowercase_with_underscores_name(name: &str) -> bool {
    let mut has_lowercase = false;

    for ch in name.chars() {
        if ch.is_ascii_lowercase() {
            has_lowercase = true;
            continue;
        }

        if ch.is_ascii_digit() || ch == '_' {
            continue;
        }

        return false;
    }

    has_lowercase
}

/// Returns true for UPPER_CASE constant identifiers.
///
/// Rule: uppercase letters/digits/underscores only, and at least one uppercase letter.
pub(crate) fn is_uppercase_constant_name(name: &str) -> bool {
    let mut has_uppercase = false;

    for ch in name.chars() {
        if ch.is_ascii_uppercase() {
            has_uppercase = true;
            continue;
        }

        if ch.is_ascii_digit() || ch == '_' {
            continue;
        }

        return false;
    }

    has_uppercase
}

/// Builds a naming warning for the given identifier/category, if style does not match policy.
pub(crate) fn naming_warning_for_identifier(
    identifier: &str,
    location: SourceLocation,
    naming_kind: IdentifierNamingKind,
) -> Option<CompilerWarning> {
    match naming_kind {
        IdentifierNamingKind::TypeLike => {
            if is_camel_case_type_name(identifier) {
                return None;
            }

            Some(CompilerWarning::new(
                &format!(
                    "'{}' should use CamelCase for struct/choice/trait/type-like names.",
                    identifier
                ),
                location,
                WarningKind::IdentifierNamingConvention,
            ))
        }
        IdentifierNamingKind::ValueLike => {
            if is_lowercase_with_underscores_name(identifier) {
                return None;
            }

            Some(CompilerWarning::new(
                &format!(
                    "'{}' should use lowercase_with_underscores for value/function/binding names.",
                    identifier
                ),
                location,
                WarningKind::IdentifierNamingConvention,
            ))
        }
        IdentifierNamingKind::TopLevelConstant => {
            if is_lowercase_with_underscores_name(identifier)
                || is_uppercase_constant_name(identifier)
            {
                return None;
            }

            Some(CompilerWarning::new(
                &format!(
                    "Top-level constant '{}' should use lowercase_with_underscores or UPPER_CASE_WITH_UNDERSCORES.",
                    identifier
                ),
                location,
                WarningKind::IdentifierNamingConvention,
            ))
        }
    }
}

/// Returns a hard error when the identifier shadows a reserved keyword.
pub(crate) fn reserved_keyword_shadow_error(
    identifier: &str,
    location: SourceLocation,
    compilation_stage: &str,
) -> CompilerError {
    let shadowed_keyword = keyword_shadow_match(identifier).unwrap_or("<unknown>");
    let mut error = CompilerError::new_rule_error(
        format!(
            "Identifier '{}' is reserved because it visually shadows language keyword '{}'. Keyword shadows are not allowed even with capitalization changes or leading underscores.",
            identifier, shadowed_keyword
        ),
        location,
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        compilation_stage.to_owned(),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        String::from(
            "Choose a name that does not match a language keyword when case and leading underscores are ignored",
        ),
    );
    error
}

pub(crate) fn ensure_not_keyword_shadow_identifier(
    identifier: &str,
    location: SourceLocation,
    compilation_stage: &str,
) -> Result<(), CompilerError> {
    if is_keyword_shadow_identifier(identifier) {
        return Err(reserved_keyword_shadow_error(
            identifier,
            location,
            compilation_stage,
        ));
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests/identifier_policy_tests.rs"]
mod identifier_policy_tests;
