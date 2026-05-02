//! Reserved trait syntax helpers for the frontend.
//!
//! WHAT: centralizes diagnostics for `must` and `This` while the trait system remains
//! intentionally unimplemented.
//! WHY: multiple parser stages need to reject the same reserved keywords with consistent wording
//! and metadata instead of each stage inventing its own fallback error.

use crate::compiler_frontend::compiler_messages::compiler_errors::{
    CompilerError, ErrorMetaDataKey, ErrorType,
};
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    TypeAnnotationContext, parse_type_annotation,
};
use crate::compiler_frontend::deferred_feature_diagnostics::deferred_feature_rule_error;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReservedTraitKeyword {
    Must,
    This,
}

impl ReservedTraitKeyword {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ReservedTraitKeyword::Must => "must",
            ReservedTraitKeyword::This => "This",
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
    compilation_stage: &'static str,
    primary_suggestion: &'static str,
) -> CompilerError {
    deferred_feature_rule_error(
        format!(
            "Keyword '{}' is reserved for traits and is deferred for Alpha.",
            keyword.as_str()
        ),
        location,
        compilation_stage,
        primary_suggestion,
    )
}

pub(crate) fn reserved_trait_declaration_error(location: SourceLocation) -> CompilerError {
    deferred_feature_rule_error(
        "Trait declarations using 'must' are reserved for traits and are deferred for Alpha.",
        location,
        "Header Parsing",
        "Use a normal declaration form until trait declarations are supported.",
    )
}

pub(crate) fn reserved_trait_dispatch_mismatch_error(
    token_kind: &TokenKind,
    location: SourceLocation,
    compilation_stage: &'static str,
    parser_context: &'static str,
) -> CompilerError {
    let mut metadata = HashMap::new();
    metadata.insert(
        ErrorMetaDataKey::CompilationStage,
        compilation_stage.to_owned(),
    );
    metadata.insert(
        ErrorMetaDataKey::PrimarySuggestion,
        String::from("This indicates parser dispatch drift. Please report this compiler bug."),
    );

    CompilerError {
        msg: format!("Reserved trait token dispatch mismatch in {parser_context}: {token_kind:?}"),
        location,
        error_type: ErrorType::Compiler,
        metadata,
    }
}

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
) -> Result<(), CompilerError> {
    // Expect 'must' keyword
    if token_stream.current_token_kind() != &TokenKind::Must {
        return Err(reserved_trait_dispatch_mismatch_error(
            token_stream.current_token_kind(),
            token_stream.current_location(),
            "Trait Syntax Parsing",
            "parse_reserved_trait_syntax",
        ));
    }
    token_stream.advance();

    // Expect ':' after 'must'
    if token_stream.current_token_kind() != &TokenKind::Colon {
        return Err(CompilerError::new_syntax_error(
            "Expected ':' after 'must' keyword in trait declaration.",
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
                return Err(CompilerError::new_syntax_error(
                    "Unexpected end of file in trait declaration. Expected method requirements followed by ';'.",
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
) -> Result<(), CompilerError> {
    // Expect method name (Symbol)
    if !matches!(token_stream.current_token_kind(), TokenKind::Symbol(_)) {
        return Err(CompilerError::new_syntax_error(
            "Expected method name in trait requirement.",
            token_stream.current_location(),
        ));
    }
    token_stream.advance();

    // Expect '|' to start parameter list
    if token_stream.current_token_kind() != &TokenKind::TypeParameterBracket {
        return Err(CompilerError::new_syntax_error(
            "Expected '|' to start parameter list in trait method requirement.",
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
                // WHAT: accepts both lowercase 'this' and capital 'This' for receiver parameter
                // WHY: maintains compatibility with existing code during transition to new grammar
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
                        return Err(CompilerError::new_syntax_error(
                            "Expected ',' or '|' after 'this' parameter.",
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
                        return Err(CompilerError::new_syntax_error(
                            "Expected ',' or '|' in parameter list.",
                            token_stream.current_location(),
                        ));
                    }
                }
            }
            TokenKind::Eof => {
                return Err(CompilerError::new_syntax_error(
                    "Unexpected end of file in trait method parameter list.",
                    token_stream.current_location(),
                ));
            }
            _ => {
                return Err(CompilerError::new_syntax_error(
                    format!(
                        "Unexpected token in parameter list: {:?}",
                        token_stream.current_token_kind()
                    ),
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
    // WHAT: detects semicolons after method requirements, which are not allowed in the new grammar
    // WHY: methods should be separated by newlines; only the trait block should end with ';'
    if token_stream.current_token_kind() == &TokenKind::End {
        // This is a semicolon after a method requirement
        // In the new grammar, this is always an error - methods should end with newlines
        // The only semicolon should be the block-closing one (after all methods)

        // To distinguish between an internal semicolon and the block-closing one:
        // - Internal semicolon: followed by more content (methods or newlines then methods)
        // - Block-closing semicolon: followed by nothing or EOF

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
                // Example: `method;` newline `;` (block-closing)
                had_newlines
            }
            _ => true, // More content follows - definitely an internal semicolon
        };

        if is_internal_semicolon {
            return Err(trait_internal_semicolon_error(semicolon_location));
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
        _ => Err(CompilerError::new_syntax_error(
            "Expected newline or ';' after trait method requirement.",
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
) -> Result<(), CompilerError> {
    // Expect '->' arrow
    if token_stream.current_token_kind() != &TokenKind::Arrow {
        return Err(CompilerError::new_syntax_error(
            "Expected '->' for return type specification.",
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
                        return Err(trait_return_trailing_comma_error(comma_location));
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
                return Err(CompilerError::new_syntax_error(
                    format!(
                        "Expected ',' or end of return list, found {:?}",
                        token_stream.current_token_kind()
                    ),
                    token_stream.current_location(),
                ));
            }
        }
    }

    Ok(())
}

/// Produces diagnostic for internal semicolons in trait method requirements.
pub(crate) fn trait_internal_semicolon_error(location: SourceLocation) -> CompilerError {
    let mut metadata = HashMap::new();
    metadata.insert(
        ErrorMetaDataKey::CompilationStage,
        String::from("Trait Syntax Parsing"),
    );
    metadata.insert(
        ErrorMetaDataKey::PrimarySuggestion,
        String::from(
            "Remove semicolons between method requirements. Use newlines to separate methods.",
        ),
    );

    CompilerError {
        msg: String::from(
            "Trait method requirements should not end with semicolons. Separate methods with newlines and end the trait block with a single ';'.",
        ),
        location,
        error_type: ErrorType::Syntax,
        metadata,
    }
}

/// Produces diagnostic for trailing commas in trait method returns.
pub(crate) fn trait_return_trailing_comma_error(location: SourceLocation) -> CompilerError {
    let mut metadata = HashMap::new();
    metadata.insert(
        ErrorMetaDataKey::CompilationStage,
        String::from("Trait Syntax Parsing"),
    );
    metadata.insert(
        ErrorMetaDataKey::PrimarySuggestion,
        String::from("Remove the trailing comma after the last return type."),
    );

    CompilerError {
        msg: String::from("Trailing comma is not allowed in trait method return lists."),
        location,
        error_type: ErrorType::Syntax,
        metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler_frontend::interned_path::InternedPath;
    use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
    use crate::compiler_frontend::tokenizer::lexer::tokenize;
    use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
    use crate::compiler_frontend::tokenizer::tokens::TokenizeMode;

    /// Helper to tokenize trait syntax for testing.
    ///
    /// WHAT: creates a token stream from source text for trait parsing tests.
    /// WHY: trait syntax parsing operates on token streams, not AST.
    fn tokenize_trait_syntax(source: &str) -> (FileTokens, StringTable) {
        let mut string_table = StringTable::new();
        let style_directives = StyleDirectiveRegistry::built_ins();
        let file_path = InternedPath::from_single_str("test.bst", &mut string_table);

        let tokens = tokenize(
            source,
            &file_path,
            TokenizeMode::Normal,
            NewlineMode::NormalizeToLf,
            &style_directives,
            &mut string_table,
            None,
        )
        .expect("tokenization should succeed");

        (tokens, string_table)
    }

    /// Helper to position token stream at 'must' keyword for trait parsing tests.
    ///
    /// WHAT: advances past ModuleStart and type name tokens to reach 'must'.
    /// WHY: trait parser expects to start at the 'must' keyword.
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
        let error = result.unwrap_err();
        assert_eq!(error.error_type, ErrorType::Syntax);
        assert!(error.msg.contains("should not end with semicolons"));
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
        let error = result.unwrap_err();
        assert_eq!(error.error_type, ErrorType::Syntax);
        assert!(error.msg.contains("Trailing comma is not allowed"));
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
        let error = result.unwrap_err();
        assert!(error.msg.contains("Expected ':' after 'must'"));
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
        let error = result.unwrap_err();
        assert!(error.msg.contains("Unexpected end of file"));
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
        let error = result.unwrap_err();
        assert!(error.msg.contains("Expected '|' to start parameter list"));
    }

    #[test]
    fn trait_internal_semicolon_error_has_correct_metadata() {
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

        let error = trait_internal_semicolon_error(location);

        assert_eq!(error.error_type, ErrorType::Syntax);
        assert!(error.msg.contains("should not end with semicolons"));
        assert_eq!(
            error.metadata.get(&ErrorMetaDataKey::CompilationStage),
            Some(&String::from("Trait Syntax Parsing"))
        );
        assert_eq!(
            error.metadata.get(&ErrorMetaDataKey::PrimarySuggestion),
            Some(&String::from(
                "Remove semicolons between method requirements. Use newlines to separate methods."
            ))
        );
    }

    #[test]
    fn trait_return_trailing_comma_error_has_correct_metadata() {
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

        let error = trait_return_trailing_comma_error(location);

        assert_eq!(error.error_type, ErrorType::Syntax);
        assert!(error.msg.contains("Trailing comma is not allowed"));
        assert_eq!(
            error.metadata.get(&ErrorMetaDataKey::CompilationStage),
            Some(&String::from("Trait Syntax Parsing"))
        );
        assert_eq!(
            error.metadata.get(&ErrorMetaDataKey::PrimarySuggestion),
            Some(&String::from(
                "Remove the trailing comma after the last return type."
            ))
        );
    }
}
