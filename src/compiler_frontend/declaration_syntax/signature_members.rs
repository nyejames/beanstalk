//! Shared `| ... |` signature-definition parsing for functions and structs.
//!
//! WHAT: owns the shared syntax for parameter/field declarations inside `| ... |` delimiters,
//! covering name, optional `~` mutability, explicit type, and optional `= default`.
//! WHY: function signatures and struct definitions use an identical syntactic form. Centralising
//! the shared parser here keeps `ast/statements/functions.rs` and `ast/statements/structs.rs`
//! free of duplicated implementation and makes the shared concept visible at the module level.
//! This module is part of `declaration_syntax`, which is the neutral home for top-level shell
//! parsers shared between the header stage and the AST stage.

use crate::ast_log;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression_until;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    TypeAnnotationContext, parse_type_annotation,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword_error, reserved_trait_keyword_or_dispatch_mismatch,
};
use crate::compiler_frontend::symbols::identifier_policy::{
    IdentifierNamingKind, ensure_not_keyword_shadow_identifier, naming_warning_for_identifier,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::syntax_errors::signature_position::check_signature_common_mistake;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
use crate::return_syntax_error;

/// Distinguishes the two syntactic contexts that share `| ... |` member parsing.
///
/// WHAT: `this` is valid only in function parameter lists (as a receiver), not in struct fields.
/// WHY: both forms share the same parser, but the set of legal names differs by context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignatureMemberContext {
    FunctionParameter,
    StructField,
    /// Payload field inside a choice variant record body.
    ///
    /// WHY: choice payload fields share the same syntactic rules as struct fields
    /// (name, optional mutability, type, optional default) but exist in a different
    /// semantic context. Kept as a distinct variant for clarity.
    ChoicePayloadField,
}

/// Parses a `| name [~]Type [= default], ... |` member list.
///
/// WHAT: shared parser for both function parameters and struct field declarations.
/// WHY: both syntactic forms are identical; one parser serves both.
///
/// Starts after the opening `|`. Stops when `TypeParameterBracket` (`|`) is reached, leaving
/// the stream positioned on the closing `|`.
pub fn parse_signature_members(
    token_stream: &mut FileTokens,
    string_table: &mut StringTable,
    expression_context: &ScopeContext,
    member_context: SignatureMemberContext,
    owner_path: &InternedPath,
) -> Result<Vec<Declaration>, CompilerError> {
    let mut members = Vec::with_capacity(1);
    let mut expecting_member = true;

    while token_stream.index < token_stream.tokens.len() {
        match token_stream.current_token_kind().to_owned() {
            TokenKind::TypeParameterBracket => {
                return Ok(members);
            }

            TokenKind::End => {
                return_syntax_error!(
                    "Unexpected end to this scope while parsing function parameters",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Struct/Parameter Parsing",
                        PrimarySuggestion => "Add closing bracket '|' for function parameters",
                        SuggestedInsertion => "|",
                    }
                )
            }

            TokenKind::Arrow | TokenKind::Colon => {
                return_syntax_error!(
                    "Function/struct parameters are missing a closing '|'.",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Struct/Parameter Parsing",
                        PrimarySuggestion => "Close the parameter list with '|' before writing '->' or ':'",
                        SuggestedInsertion => "|",
                    }
                )
            }

            TokenKind::Symbol(arg_name) => {
                if !expecting_member {
                    return_syntax_error!(
                        "Should have a comma to separate arguments",
                        token_stream.current_location(),
                        {
                            CompilationStage => "Struct/Parameter Parsing",
                            PrimarySuggestion => "Add ',' between struct fields or function parameters",
                            SuggestedInsertion => ",",
                        }
                    )
                }

                let member = parse_signature_member(
                    token_stream,
                    owner_path.append(arg_name),
                    expression_context,
                    string_table,
                    false,
                    member_context,
                )?;

                members.push(member);

                expecting_member = false;
            }

            TokenKind::This if member_context == SignatureMemberContext::FunctionParameter => {
                if !expecting_member {
                    return_syntax_error!(
                        "Should have a comma to separate arguments",
                        token_stream.current_location(),
                        {
                            CompilationStage => "Struct/Parameter Parsing",
                            PrimarySuggestion => "Add ',' between function parameters",
                            SuggestedInsertion => ",",
                        }
                    )
                }

                let this_id = string_table.intern("this");
                let member = parse_signature_member(
                    token_stream,
                    owner_path.append(this_id),
                    expression_context,
                    string_table,
                    true,
                    member_context,
                )?;

                members.push(member);
                expecting_member = false;
            }

            TokenKind::This => {
                return_syntax_error!(
                    "'this' is reserved for method receiver parameters and cannot be used as a struct field name.",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Struct/Parameter Parsing",
                        PrimarySuggestion => "Rename this field or use 'this' only as the first parameter of a receiver method",
                    }
                )
            }

            TokenKind::Comma => {
                token_stream.advance();
                expecting_member = true;
            }

            TokenKind::Must | TokenKind::TraitThis => {
                let keyword = reserved_trait_keyword_or_dispatch_mismatch(
                    token_stream.current_token_kind(),
                    token_stream.current_location(),
                    "Struct/Parameter Parsing",
                    "signature member parsing",
                )?;

                return Err(reserved_trait_keyword_error(
                    keyword,
                    token_stream.current_location(),
                    "Struct/Parameter Parsing",
                    "Use a normal parameter or field name until traits are implemented",
                ));
            }

            TokenKind::Newline => {
                token_stream.advance();
            }

            TokenKind::Eof => {
                return_syntax_error!(
                    "Unexpected end of file. Type definition is missing a closing bracket. Expected: '|'",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Struct/Parameter Parsing",
                        PrimarySuggestion => "Add closing bracket '|' to complete the type definition",
                        SuggestedInsertion => "|",
                    }
                )
            }

            _ => {
                if let Some(error) = check_signature_common_mistake(token_stream) {
                    return Err(error);
                }

                return_syntax_error!(
                    format!(
                        "Unexpected token used in function arguments: {:?}",
                        token_stream.current_token_kind()
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => "Struct/Parameter Parsing",
                        PrimarySuggestion => "Use valid parameter syntax: name Type or name ~Type for mutable",
                    }
                )
            }
        }
    }

    Ok(members)
}

/// Parses a single `name [~]Type [= default]` member declaration inside a `| ... |` list.
///
/// WHAT: the canonical parser for one parameter or struct field declaration.
/// WHY: function parameters and struct fields share this syntax; a single implementation
/// avoids drift between the two forms.
///
/// Starts with the stream positioned on the name token (already matched by the caller).
fn parse_signature_member(
    token_stream: &mut FileTokens,
    full_name: InternedPath,
    expression_context: &ScopeContext,
    string_table: &mut StringTable,
    allow_reserved_this: bool,
    member_context: SignatureMemberContext,
) -> Result<Declaration, CompilerError> {
    let member_name = full_name
        .name()
        .map(|id| string_table.resolve(id).to_owned())
        .unwrap_or_else(|| String::from("<unknown>"));

    if !allow_reserved_this || member_name != "this" {
        ensure_not_keyword_shadow_identifier(
            &member_name,
            token_stream.current_location(),
            "Struct/Parameter Parsing",
        )?;
    }

    if let Some(warning) = naming_warning_for_identifier(
        &member_name,
        token_stream.current_location(),
        IdentifierNamingKind::ValueLike,
    ) {
        expression_context.emit_warning(warning);
    }

    // Move past the name.
    token_stream.advance();

    let mut value_mode = ValueMode::ImmutableOwned;

    if token_stream.current_token_kind() == &TokenKind::Mutable {
        token_stream.advance();
        value_mode = ValueMode::MutableOwned;
    };

    if member_context == SignatureMemberContext::ChoicePayloadField
        && value_mode == ValueMode::MutableOwned
    {
        return_syntax_error!(
            "Choice payload fields cannot be marked mutable. Mutability belongs to bindings, not variant payload declarations.",
            token_stream.current_location(),
            {
                CompilationStage => "Choice Declaration",
                PrimarySuggestion => "Remove the '~' symbol from the payload field",
            }
        );
    }

    while token_stream.current_token_kind() == &TokenKind::Newline {
        token_stream.advance();
    }

    let parsed_type =
        parse_type_annotation(token_stream, TypeAnnotationContext::SignatureParameter)?;
    let mut data_type = parsed_type;

    match token_stream.current_token_kind() {
        TokenKind::Assign => {
            token_stream.advance();
            if member_context == SignatureMemberContext::ChoicePayloadField {
                return_syntax_error!(
                    "Choice payload fields cannot have default values.",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Choice Declaration",
                        PrimarySuggestion => "Remove the '= value' default from the payload field",
                    }
                );
            }
        }

        TokenKind::Comma
        | TokenKind::Eof
        | TokenKind::Newline
        | TokenKind::TypeParameterBracket => {
            ast_log!(
                "Created new parameter of type: ",
                data_type.display_with_table(string_table)
            );
            return Ok(Declaration {
                id: full_name,
                value: Expression::new(
                    ExpressionKind::NoValue,
                    token_stream.current_location(),
                    data_type,
                    value_mode,
                ),
            });
        }

        TokenKind::As => {
            return_syntax_error!(
                "`as` is not valid in function signatures or struct fields. It is only supported in type aliases, import clauses, and choice payload patterns.",
                token_stream.current_location(),
                {
                    CompilationStage => "Parameter Parsing",
                    PrimarySuggestion => "Remove `as` from the parameter/field declaration",
                }
            )
        }

        _ => {
            return_syntax_error!(
                format!(
                    "Unexpected Token: {:?}. Are you trying to reference a variable that doesn't exist yet?",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location(),
                {
                    CompilationStage => "Parameter Parsing",
                    PrimarySuggestion => "Check that all referenced variables are declared before use",
                }
            )
        }
    }

    // At header stage, skip default expressions that start with a symbol not visible in the
    // current context (e.g. function declarations or symbols from other files not yet parsed).
    // Such defaults cannot be fully parsed here without producing false "undefined variable"
    // errors. A synthetic Reference is returned so that AST-stage validation in
    // resolve_struct_field_types can still catch non-compile-time defaults (e.g. functions).
    // Literals and symbols that ARE in scope (constant placeholders) parse normally.
    if expression_context.kind == ContextKind::ConstantHeader
        && let TokenKind::Symbol(name) = token_stream.current_token_kind().clone()
        && expression_context.get_reference(&name).is_none()
    {
        // Build a synthetic path so the AST-stage name lookup can find the declaration
        // and determine whether it is a compile-time constant or not.
        let ref_path = expression_context.scope.append(name);
        let ref_location = token_stream.current_location();
        let mut depth: usize = 0;
        loop {
            match token_stream.current_token_kind() {
                TokenKind::OpenParenthesis | TokenKind::OpenCurly => {
                    depth += 1;
                    token_stream.advance();
                }
                TokenKind::CloseParenthesis | TokenKind::CloseCurly => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    token_stream.advance();
                }
                TokenKind::TypeParameterBracket | TokenKind::Comma if depth == 0 => break,
                TokenKind::Eof => break,
                _ => {
                    token_stream.advance();
                }
            }
        }
        return Ok(Declaration {
            id: full_name,
            value: Expression::new(
                ExpressionKind::Reference(ref_path),
                ref_location,
                data_type,
                value_mode,
            ),
        });
    }

    let mut parameter_context = expression_context.to_owned();
    parameter_context.expected_result_types = vec![data_type.clone()];

    let parsed_expr = create_expression_until(
        token_stream,
        &parameter_context,
        &mut data_type,
        &value_mode,
        &[TokenKind::TypeParameterBracket, TokenKind::Comma],
        string_table,
    )?;

    ast_log!(
        "Created new ",
        #value_mode,
        " variable of type: ",
        data_type.display_with_table(string_table)
    );

    Ok(Declaration {
        id: full_name,
        value: parsed_expr,
    })
}
