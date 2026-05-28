#![allow(clippy::result_large_err)]

//! Reserved trait syntax helpers for the frontend.
//!
//! WHAT: centralizes diagnostics for `must` and `This` while the trait system remains
//! intentionally unimplemented.
//! WHY: multiple parser stages need to reject the same reserved keywords with typed diagnostics
//! while keeping parser-dispatch mismatches on the internal compiler-error path.

use crate::compiler_frontend::compiler_messages::compiler_errors::{
    CompilerError, CompilerErrorMetadataKey, ErrorType,
};
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, DeferredFeatureReason};
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    TypeAnnotationContext, parse_type_annotation,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReservedTraitKeyword {
    Must,
    This,
}

impl ReservedTraitKeyword {
    fn deferred_feature_reason(self) -> DeferredFeatureReason {
        match self {
            ReservedTraitKeyword::Must => DeferredFeatureReason::ReservedTraitMustKeyword,
            ReservedTraitKeyword::This => DeferredFeatureReason::ReservedTraitThisKeyword,
        }
    }
}

pub(crate) fn reserved_trait_keyword(token_kind: &TokenKind) -> Option<ReservedTraitKeyword> {
    match token_kind {
        TokenKind::Must => Some(ReservedTraitKeyword::Must),
        TokenKind::TraitThis => Some(ReservedTraitKeyword::This),
        _ => None,
    }
}

/// Resolves a reserved trait keyword in contexts that already dispatched on reserved tokens.
///
/// WHAT: converts `must` / `This` token kinds into their reserved-keyword enum variant.
/// WHY: parser dispatch drift should return a structured internal compiler diagnostic instead of
/// relying on nearby `expect(...)` assumptions.
pub(crate) fn reserved_trait_keyword_or_dispatch_mismatch(
    token_kind: &TokenKind,
    location: SourceLocation,
    compilation_stage: &'static str,
    parser_context: &'static str,
) -> Result<ReservedTraitKeyword, CompilerError> {
    reserved_trait_keyword(token_kind).ok_or_else(|| {
        reserved_trait_dispatch_mismatch_error(
            token_kind,
            location,
            compilation_stage,
            parser_context,
        )
    })
}

pub(crate) fn reserved_trait_keyword_error(
    keyword: ReservedTraitKeyword,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::deferred_feature_reason(keyword.deferred_feature_reason(), location)
}

pub(crate) fn reserved_trait_dispatch_mismatch_error(
    token_kind: &TokenKind,
    location: SourceLocation,
    compilation_stage: &'static str,
    parser_context: &'static str,
) -> CompilerError {
    let mut metadata = HashMap::new();
    metadata.insert(
        CompilerErrorMetadataKey::CompilationStage,
        compilation_stage.to_owned(),
    );
    metadata.insert(
        CompilerErrorMetadataKey::PrimarySuggestion,
        String::from("This indicates parser dispatch drift. Please report this compiler bug."),
    );

    let mut error = CompilerError::new(
        format!("Reserved trait token dispatch mismatch in {parser_context}: {token_kind:?}"),
        location,
        ErrorType::Compiler,
    );
    error.metadata = metadata;
    error
}

pub(crate) fn reserved_trait_declaration_diagnostic(
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::deferred_feature_reason(DeferredFeatureReason::TraitDeclaration, location)
}

// ------------------------
//  Parse reserved traits
// ------------------------

/// Parses the reserved trait syntax block and produces a diagnostic.
///
/// Grammar: `TypeName must: MethodRequirement* ;`
///
/// WHAT: validates the structure of reserved trait syntax without lowering to AST/HIR.
/// WHY: traits are deferred but the grammar must be recognized for future implementation.
pub(crate) fn parse_reserved_trait_syntax(
    token_stream: &mut FileTokens,
    _type_name: StringId,
    _string_table: &StringTable,
) -> Result<(), CompilerDiagnostic> {
    if token_stream.current_token_kind() != &TokenKind::Must {
        return Err(CompilerDiagnostic::expected_token(
            TokenKind::Must,
            Some(token_stream.current_token_kind().clone()),
            token_stream.current_location(),
        ));
    }
    token_stream.advance();

    // Expect ':' after 'must'
    if token_stream.current_token_kind() != &TokenKind::Colon {
        return Err(CompilerDiagnostic::unexpected_token_in_declaration(
            token_stream.current_location(),
        ));
    }
    token_stream.advance();

    // Skip initial newlines
    token_stream.skip_newlines();

    // Parse method requirements until we hit the block-closing semicolon
    loop {
        match token_stream.current_token_kind() {
            TokenKind::End => {
                // Block-closing semicolon
                token_stream.advance();
                break;
            }
            TokenKind::Eof => {
                return Err(CompilerDiagnostic::unexpected_end_of_file(
                    None,
                    token_stream.current_location(),
                ));
            }
            TokenKind::Newline => {
                // Skip newlines between method requirements
                token_stream.skip_newlines();
            }
            _ => {
                // Parse a method requirement
                parse_trait_method_requirement(token_stream, _string_table)?;
            }
        }
    }

    Ok(())
}

/// Parses a single method requirement within a trait block.
///
/// Grammar: `method_name |params| [-> ReturnType (, ReturnType)*]`
///
/// WHAT: validates method signature structure without creating AST nodes.
/// WHY: method requirements follow function signature syntax but remain reserved.
fn parse_trait_method_requirement(
    token_stream: &mut FileTokens,
    _string_table: &StringTable,
) -> Result<(), CompilerDiagnostic> {
    // Expect method name (Symbol)
    if !matches!(token_stream.current_token_kind(), TokenKind::Symbol(_)) {
        return Err(CompilerDiagnostic::unexpected_token_in_declaration(
            token_stream.current_location(),
        ));
    }
    token_stream.advance();

    // Expect '|' to start parameter list
    if token_stream.current_token_kind() != &TokenKind::TypeParameterBracket {
        return Err(CompilerDiagnostic::unexpected_token_in_declaration(
            token_stream.current_location(),
        ));
    }
    token_stream.advance();

    // Parse parameters until closing '|'
    loop {
        match token_stream.current_token_kind() {
            TokenKind::TypeParameterBracket => {
                // Closing '|'
                token_stream.advance();
                break;
            }
            TokenKind::This | TokenKind::TraitThis => {
                // Receiver parameter 'this' or 'This' - no type annotation needed
                token_stream.advance();

                // Optional '~' for mutable receiver
                if token_stream.current_token_kind() == &TokenKind::Mutable {
                    token_stream.advance();
                }

                // Expect comma or closing '|'
                match token_stream.current_token_kind() {
                    TokenKind::Comma => {
                        token_stream.advance();
                    }
                    TokenKind::TypeParameterBracket => {
                        // Will be handled in next iteration
                    }
                    _ => {
                        return Err(CompilerDiagnostic::unexpected_token_in_declaration(
                            token_stream.current_location(),
                        ));
                    }
                }
            }
            TokenKind::Symbol(_) => {
                // Parameter name
                token_stream.advance();

                // Optional '~' for mutable parameters
                if token_stream.current_token_kind() == &TokenKind::Mutable {
                    token_stream.advance();
                }

                // Parse parameter type
                parse_type_annotation(token_stream, TypeAnnotationContext::SignatureParameter)?;

                // Expect comma or closing '|'
                match token_stream.current_token_kind() {
                    TokenKind::Comma => {
                        token_stream.advance();
                    }
                    TokenKind::TypeParameterBracket => {
                        // Will be handled in next iteration
                    }
                    _ => {
                        return Err(CompilerDiagnostic::unexpected_token_in_declaration(
                            token_stream.current_location(),
                        ));
                    }
                }
            }
            TokenKind::Eof => {
                return Err(CompilerDiagnostic::unexpected_end_of_file(
                    None,
                    token_stream.current_location(),
                ));
            }
            _ => {
                return Err(CompilerDiagnostic::unexpected_token_in_declaration(
                    token_stream.current_location(),
                ));
            }
        }
    }

    // Check for optional return signature
    if token_stream.current_token_kind() == &TokenKind::Arrow {
        parse_trait_method_returns(token_stream, _string_table)?;
    }

    // Check for internal semicolon (which is invalid)
    if token_stream.current_token_kind() == &TokenKind::End {
        let semicolon_location = token_stream.current_location();
        token_stream.advance();

        // Skip any newlines after the semicolon
        let had_newlines = token_stream.current_token_kind() == &TokenKind::Newline;
        token_stream.skip_newlines();

        // Check what comes after the semicolon (and any newlines)
        let is_internal_semicolon = match token_stream.current_token_kind() {
            TokenKind::Eof => false, // Block-closing semicolon at end of file
            TokenKind::End => {
                // Another semicolon - this means the first one was internal
                had_newlines
            }
            _ => true, // More content follows - definitely an internal semicolon
        };

        if is_internal_semicolon {
            return Err(trait_internal_semicolon_diagnostic(semicolon_location));
        }

        // This is the block-closing semicolon - put the token back
        token_stream.index -= 1;
        if token_stream.index > 0 && had_newlines {
            // Skip back over any newlines we consumed
            while token_stream.index > 0
                && matches!(
                    token_stream.tokens[token_stream.index - 1].kind,
                    TokenKind::Newline
                )
            {
                token_stream.index -= 1;
            }
        }
    }

    // Expect newline or block-closing semicolon
    match token_stream.current_token_kind() {
        TokenKind::Newline | TokenKind::End => {
            // Valid - method requirement ends here
            Ok(())
        }
        _ => Err(CompilerDiagnostic::unexpected_token_in_declaration(
            token_stream.current_location(),
        )),
    }
}

/// Parses a comma-separated return type list for trait methods.
///
/// Grammar: `-> ReturnType (, ReturnType)*`
///
/// WHAT: validates return list syntax including trailing comma rejection.
/// WHY: return lists must follow the same rules as function returns.
fn parse_trait_method_returns(
    token_stream: &mut FileTokens,
    _string_table: &StringTable,
) -> Result<(), CompilerDiagnostic> {
    // Expect '->' arrow
    if token_stream.current_token_kind() != &TokenKind::Arrow {
        return Err(CompilerDiagnostic::unexpected_token_in_declaration(
            token_stream.current_location(),
        ));
    }
    token_stream.advance();

    // Parse at least one return type
    parse_type_annotation(token_stream, TypeAnnotationContext::SignatureReturn)?;

    // Parse additional return types separated by commas
    loop {
        match token_stream.current_token_kind() {
            TokenKind::Comma => {
                let comma_location = token_stream.current_location();
                token_stream.advance();

                // Check for trailing comma (comma followed by newline or semicolon)
                match token_stream.current_token_kind() {
                    TokenKind::Newline | TokenKind::End => {
                        return Err(trait_return_trailing_comma_diagnostic(comma_location));
                    }
                    _ => {
                        // Parse next return type
                        parse_type_annotation(
                            token_stream,
                            TypeAnnotationContext::SignatureReturn,
                        )?;
                    }
                }
            }
            TokenKind::Newline | TokenKind::End => {
                // End of return list
                break;
            }
            _ => {
                return Err(CompilerDiagnostic::unexpected_token_in_declaration(
                    token_stream.current_location(),
                ));
            }
        }
    }

    Ok(())
}

/// Produces diagnostic for internal semicolons in trait method requirements.
pub(crate) fn trait_internal_semicolon_diagnostic(location: SourceLocation) -> CompilerDiagnostic {
    CompilerDiagnostic::unexpected_token_in_declaration(location)
}

/// Produces diagnostic for trailing commas in trait method returns.
pub(crate) fn trait_return_trailing_comma_diagnostic(
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::unexpected_trailing_comma(location)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler_frontend::compiler_messages::DiagnosticSeverity;
    use crate::compiler_frontend::interned_path::InternedPath;
    use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
    use crate::compiler_frontend::tokenizer::lexer::tokenize;
    use crate::compiler_frontend::tokenizer::tokens::TokenizeMode;

    /// Helper to tokenize trait syntax for testing.
    fn tokenize_trait_syntax(source: &str) -> (FileTokens, StringTable) {
        let mut string_table = StringTable::new();
        let style_directives = StyleDirectiveRegistry::built_ins();
        let file_path = InternedPath::from_single_str("test.bst", &mut string_table);

        let tokens = tokenize(
            source,
            &file_path,
            TokenizeMode::Normal,
            &style_directives,
            &mut string_table,
            None,
        )
        .expect("tokenization should succeed");

        (tokens, string_table)
    }

    /// Helper to position token stream at 'must' keyword for trait parsing tests.
    fn skip_to_must_keyword(tokens: &mut FileTokens) {
        tokens.advance(); // Skip ModuleStart
        tokens.advance(); // Skip type name
    }

    #[test]
    fn parses_valid_trait_syntax_without_internal_semicolons() {
        let source = "Drawable must:\n    draw |this|\n    move |this, x Int, y Int|\n;";
        let (mut tokens, mut string_table) = tokenize_trait_syntax(source);
        skip_to_must_keyword(&mut tokens);

        let type_name = string_table.intern("Drawable");
        let result = parse_reserved_trait_syntax(&mut tokens, type_name, &string_table);

        assert!(
            result.is_ok(),
            "Valid trait syntax should parse successfully"
        );
    }

    #[test]
    fn parses_valid_trait_methods_with_return_types() {
        let source = "Comparable must:\n    compare |this, other Int| -> Int\n    equals |this, other Int| -> Bool\n;";
        let (mut tokens, mut string_table) = tokenize_trait_syntax(source);
        skip_to_must_keyword(&mut tokens);

        let type_name = string_table.intern("Comparable");
        let result = parse_reserved_trait_syntax(&mut tokens, type_name, &string_table);

        assert!(
            result.is_ok(),
            "Trait methods with return types should parse successfully"
        );
    }

    #[test]
    fn parses_valid_trait_methods_with_multiple_return_types() {
        let source = "Parser must:\n    parse |input String| -> String, Bool\n;";
        let (mut tokens, mut string_table) = tokenize_trait_syntax(source);
        skip_to_must_keyword(&mut tokens);

        let type_name = string_table.intern("Parser");
        let result = parse_reserved_trait_syntax(&mut tokens, type_name, &string_table);

        assert!(
            result.is_ok(),
            "Trait methods with multiple return types should parse successfully"
        );
    }

    #[test]
    fn rejects_internal_semicolons_in_trait_methods() {
        let source = "Drawable must:\n    draw |this|;\n    move |this, x Int, y Int|;\n;";
        let (mut tokens, mut string_table) = tokenize_trait_syntax(source);
        skip_to_must_keyword(&mut tokens);

        let type_name = string_table.intern("Drawable");
        let result = parse_reserved_trait_syntax(&mut tokens, type_name, &string_table);

        assert!(result.is_err(), "Internal semicolons should be rejected");
        let diagnostic = result.unwrap_err();
        assert_eq!(
            diagnostic.kind.descriptor().code,
            "BST-SYNTAX-0013",
            "Expected UnexpectedTokenInDeclaration code"
        );
    }

    #[test]
    fn rejects_trailing_comma_in_trait_method_returns() {
        let source = "Parser must:\n    parse |input String| -> String, Bool,\n;";
        let (mut tokens, mut string_table) = tokenize_trait_syntax(source);
        skip_to_must_keyword(&mut tokens);

        let type_name = string_table.intern("Parser");
        let result = parse_reserved_trait_syntax(&mut tokens, type_name, &string_table);

        assert!(
            result.is_err(),
            "Trailing comma in returns should be rejected"
        );
        let diagnostic = result.unwrap_err();
        assert_eq!(
            diagnostic.kind.descriptor().code,
            "BST-SYNTAX-0003",
            "Expected UnexpectedTrailingComma code"
        );
    }

    #[test]
    fn parses_trait_method_without_parameters() {
        let source = "Resettable must:\n    reset |this|\n;";
        let (mut tokens, mut string_table) = tokenize_trait_syntax(source);
        skip_to_must_keyword(&mut tokens);

        let type_name = string_table.intern("Resettable");
        let result = parse_reserved_trait_syntax(&mut tokens, type_name, &string_table);

        assert!(
            result.is_ok(),
            "Trait method with only 'this' parameter should parse successfully"
        );
    }

    #[test]
    fn parses_trait_method_without_return_type() {
        let source = "Drawable must:\n    draw |this|\n;";
        let (mut tokens, mut string_table) = tokenize_trait_syntax(source);
        skip_to_must_keyword(&mut tokens);

        let type_name = string_table.intern("Drawable");
        let result = parse_reserved_trait_syntax(&mut tokens, type_name, &string_table);

        assert!(
            result.is_ok(),
            "Trait method without return type should parse successfully"
        );
    }

    #[test]
    fn parses_empty_trait_block() {
        let source = "Empty must:\n;";
        let (mut tokens, mut string_table) = tokenize_trait_syntax(source);
        skip_to_must_keyword(&mut tokens);

        let type_name = string_table.intern("Empty");
        let result = parse_reserved_trait_syntax(&mut tokens, type_name, &string_table);

        assert!(
            result.is_ok(),
            "Empty trait block should parse successfully"
        );
    }

    #[test]
    fn rejects_missing_colon_after_must() {
        let source = "Drawable must\n    draw |this|\n;";
        let (mut tokens, mut string_table) = tokenize_trait_syntax(source);
        skip_to_must_keyword(&mut tokens);

        let type_name = string_table.intern("Drawable");
        let result = parse_reserved_trait_syntax(&mut tokens, type_name, &string_table);

        assert!(
            result.is_err(),
            "Missing colon after 'must' should be rejected"
        );
        let diagnostic = result.unwrap_err();
        assert_eq!(
            diagnostic.kind.descriptor().code,
            "BST-SYNTAX-0013",
            "Expected UnexpectedTokenInDeclaration code"
        );
    }

    #[test]
    fn rejects_missing_block_closing_semicolon() {
        let source = "Drawable must:\n    draw |this|\n";
        let (mut tokens, mut string_table) = tokenize_trait_syntax(source);
        skip_to_must_keyword(&mut tokens);

        let type_name = string_table.intern("Drawable");
        let result = parse_reserved_trait_syntax(&mut tokens, type_name, &string_table);

        assert!(
            result.is_err(),
            "Missing block-closing semicolon should be rejected"
        );
        let diagnostic = result.unwrap_err();
        assert_eq!(
            diagnostic.kind.descriptor().code,
            "BST-SYNTAX-0017",
            "Expected UnexpectedEndOfFile code"
        );
    }

    #[test]
    fn parses_trait_method_with_mutable_parameters() {
        let source = "Modifier must:\n    modify |this, data ~String|\n;";
        let (mut tokens, mut string_table) = tokenize_trait_syntax(source);
        skip_to_must_keyword(&mut tokens);

        let type_name = string_table.intern("Modifier");
        let result = parse_reserved_trait_syntax(&mut tokens, type_name, &string_table);

        assert!(
            result.is_ok(),
            "Trait method with mutable parameters should parse successfully"
        );
    }

    #[test]
    fn parses_trait_method_with_multiple_parameters() {
        let source = "Calculator must:\n    add |this, a Int, b Int, c Int| -> Int\n;";
        let (mut tokens, mut string_table) = tokenize_trait_syntax(source);
        skip_to_must_keyword(&mut tokens);

        let type_name = string_table.intern("Calculator");
        let result = parse_reserved_trait_syntax(&mut tokens, type_name, &string_table);

        assert!(
            result.is_ok(),
            "Trait method with multiple parameters should parse successfully"
        );
    }

    #[test]
    fn rejects_missing_parameter_list_opening() {
        let source = "Drawable must:\n    draw this|\n;";
        let (mut tokens, mut string_table) = tokenize_trait_syntax(source);
        skip_to_must_keyword(&mut tokens);

        let type_name = string_table.intern("Drawable");
        let result = parse_reserved_trait_syntax(&mut tokens, type_name, &string_table);

        assert!(
            result.is_err(),
            "Missing '|' to start parameter list should be rejected"
        );
        let diagnostic = result.unwrap_err();
        assert_eq!(
            diagnostic.kind.descriptor().code,
            "BST-SYNTAX-0013",
            "Expected UnexpectedTokenInDeclaration code"
        );
    }

    #[test]
    fn trait_internal_semicolon_diagnostic_has_correct_kind() {
        let location = SourceLocation {
            scope: InternedPath::new(),
            start_pos: crate::compiler_frontend::tokenizer::tokens::CharPosition {
                line_number: 1,
                char_column: 0,
            },
            end_pos: crate::compiler_frontend::tokenizer::tokens::CharPosition {
                line_number: 1,
                char_column: 10,
            },
        };

        let diagnostic = trait_internal_semicolon_diagnostic(location);

        assert_eq!(
            diagnostic.kind.descriptor().code,
            "BST-SYNTAX-0013",
            "Expected UnexpectedTokenInDeclaration code"
        );
        assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    }

    #[test]
    fn trait_return_trailing_comma_diagnostic_has_correct_kind() {
        let location = SourceLocation {
            scope: InternedPath::new(),
            start_pos: crate::compiler_frontend::tokenizer::tokens::CharPosition {
                line_number: 1,
                char_column: 0,
            },
            end_pos: crate::compiler_frontend::tokenizer::tokens::CharPosition {
                line_number: 1,
                char_column: 10,
            },
        };

        let diagnostic = trait_return_trailing_comma_diagnostic(location);

        assert_eq!(
            diagnostic.kind.descriptor().code,
            "BST-SYNTAX-0003",
            "Expected UnexpectedTrailingComma code"
        );
        assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    }
}
