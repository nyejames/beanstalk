//! Shared frontend type-annotation syntax and named-type resolution helpers.
//!
//! WHAT: owns parsing/serialization of explicit type annotations and recursive
//! resolution of `NamedType` placeholders.
//! WHY: declaration parsing, signature parsing, and AST type-resolution all
//! used to maintain parallel implementations that drifted in diagnostics and
//! behavior.
//!
//! This module owns:
//! - token-to-type annotation parsing for declaration/signature contexts
//! - optional suffix (`?`) annotation rules
//! - recursive `NamedType` resolution with consistent unknown-type diagnostics
//! - annotation token emission helpers used by header/declaration plumbing
//!
//! This module does NOT own:
//! - declaration/statement-level semantics (mutability rules, initializer rules)
//! - expression typing/coercion policy
//! - call-site/feature-specific diagnostic framing outside type syntax itself

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::deferred_feature_diagnostics::deferred_feature_rule_error;
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword, reserved_trait_keyword_error,
};
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::return_syntax_error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeAnnotationContext {
    DeclarationTarget,
    SignatureParameter,
    SignatureReturn,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TypeAnnotationSyntax {
    pub(crate) data_type: DataType,
}

impl TypeAnnotationSyntax {
    pub(crate) fn inferred() -> Self {
        Self {
            data_type: DataType::Inferred,
        }
    }

    pub(crate) fn has_explicit_type(&self) -> bool {
        !matches!(self.data_type, DataType::Inferred)
    }
}

pub(crate) fn parse_type_annotation(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
) -> Result<TypeAnnotationSyntax, CompilerError> {
    if matches!(context, TypeAnnotationContext::DeclarationTarget)
        && matches!(
            token_stream.current_token_kind(),
            TokenKind::Assign | TokenKind::Newline | TokenKind::Comma
        )
    {
        return Ok(TypeAnnotationSyntax::inferred());
    }

    let parsed_type = parse_required_type(token_stream, context)?;
    Ok(TypeAnnotationSyntax {
        data_type: parsed_type,
    })
}

fn parse_required_type(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
) -> Result<DataType, CompilerError> {
    let parsed_type = match token_stream.current_token_kind() {
        TokenKind::DatatypeInt => {
            token_stream.advance();
            DataType::Int
        }
        TokenKind::DatatypeFloat => {
            token_stream.advance();
            DataType::Float
        }
        TokenKind::DatatypeBool => {
            token_stream.advance();
            DataType::Bool
        }
        TokenKind::DatatypeString => {
            token_stream.advance();
            DataType::StringSlice
        }
        TokenKind::DatatypeChar => {
            token_stream.advance();
            DataType::Char
        }
        TokenKind::DatatypeNone => {
            let (message, stage, suggestion) = none_type_annotation_error(context);
            return_syntax_error!(
                message,
                token_stream.current_location(),
                {
                    CompilationStage => stage,
                    PrimarySuggestion => suggestion,
                }
            )
        }
        TokenKind::Must | TokenKind::TraitThis => {
            let keyword = reserved_trait_keyword(token_stream.current_token_kind())
                .expect("reserved trait token should map to a keyword");

            let (stage, suggestion) = reserved_trait_type_annotation_error(context);
            return Err(reserved_trait_keyword_error(
                keyword,
                token_stream.current_location(),
                stage,
                suggestion,
            ));
        }
        TokenKind::OpenCurly => parse_collection_type(token_stream, context)?,
        TokenKind::Symbol(type_name) => {
            let type_name = *type_name;
            token_stream.advance();
            DataType::NamedType(type_name)
        }
        TokenKind::Colon if matches!(context, TypeAnnotationContext::DeclarationTarget) => {
            return Err(deferred_feature_rule_error(
                "Labeled scopes are deferred for Alpha.",
                token_stream.current_location(),
                "Variable Declaration",
                "Remove the label and use supported control flow syntax.",
            ));
        }
        other
            if matches!(context, TypeAnnotationContext::DeclarationTarget)
                && matches!(
                    other,
                    TokenKind::Dot
                        | TokenKind::AddAssign
                        | TokenKind::SubtractAssign
                        | TokenKind::DivideAssign
                        | TokenKind::MultiplyAssign
                ) =>
        {
            return_syntax_error!(
                format!(
                    "Invalid token '{other:?}' after declaration name. Expected a type or assignment operator.",
                ),
                token_stream.current_location(),
                {
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Use a type declaration (Int, String, etc.) or assignment operator '='",
                }
            )
        }
        _ => {
            let (message, stage, suggestion) = expected_type_error(context);
            return_syntax_error!(
                message,
                token_stream.current_location(),
                {
                    CompilationStage => stage,
                    PrimarySuggestion => suggestion,
                }
            )
        }
    };

    parse_optional_type_suffix(token_stream, parsed_type, context)
}

fn parse_collection_type(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
) -> Result<DataType, CompilerError> {
    token_stream.advance();

    let inner_type = if token_stream.current_token_kind() == &TokenKind::CloseCurly {
        DataType::Inferred
    } else {
        parse_required_type(token_stream, context)?
    };

    if token_stream.current_token_kind() != &TokenKind::CloseCurly {
        let stage = compilation_stage(context);
        return_syntax_error!(
            "Missing closing curly brace for collection type declaration",
            token_stream.current_location(),
            {
                CompilationStage => stage,
                PrimarySuggestion => "Add '}' to close the collection type declaration",
                SuggestedInsertion => "}",
            }
        )
    }

    token_stream.advance();

    Ok(DataType::Collection(
        Box::new(inner_type),
        crate::compiler_frontend::datatypes::Ownership::ImmutableOwned,
    ))
}

fn parse_optional_type_suffix(
    token_stream: &mut FileTokens,
    parsed_type: DataType,
    context: TypeAnnotationContext,
) -> Result<DataType, CompilerError> {
    if token_stream.current_token_kind() != &TokenKind::QuestionMark {
        return Ok(parsed_type);
    }

    if matches!(parsed_type, DataType::Option(_)) {
        let stage = compilation_stage(context);
        let duplicate_message = if matches!(context, TypeAnnotationContext::DeclarationTarget) {
            "Duplicate optional marker '?' in declaration type annotation"
        } else {
            "Duplicate optional marker '?' in type declaration"
        };

        return_syntax_error!(
            duplicate_message,
            token_stream.current_location(),
            {
                CompilationStage => stage,
                PrimarySuggestion => "Use a single '?' suffix for optional types",
            }
        );
    }

    token_stream.advance();
    if token_stream.current_token_kind() == &TokenKind::QuestionMark {
        let stage = compilation_stage(context);
        let duplicate_message = if matches!(context, TypeAnnotationContext::DeclarationTarget) {
            "Duplicate optional marker '?' in declaration type annotation"
        } else {
            "Duplicate optional marker '?' in type declaration"
        };

        return_syntax_error!(
            duplicate_message,
            token_stream.current_location(),
            {
                CompilationStage => stage,
                PrimarySuggestion => "Use a single '?' suffix for optional types",
            }
        );
    }

    Ok(DataType::Option(Box::new(parsed_type)))
}

fn none_type_annotation_error(
    context: TypeAnnotationContext,
) -> (&'static str, &'static str, &'static str) {
    match context {
        TypeAnnotationContext::DeclarationTarget => (
            "none is not a valid declaration type annotation",
            "Variable Declaration",
            "Use an optional type like 'String?' and assign 'none' as the value",
        ),
        TypeAnnotationContext::SignatureParameter => (
            "None is not a valid parameter type",
            "Parameter Type Parsing",
            "Use a concrete parameter type such as Int, String, Float, Bool, or a collection type",
        ),
        TypeAnnotationContext::SignatureReturn => (
            "None is not a valid function return type",
            "Function Signature Parsing",
            "Functions without return values should omit the return signature entirely",
        ),
    }
}

fn reserved_trait_type_annotation_error(
    context: TypeAnnotationContext,
) -> (&'static str, &'static str) {
    match context {
        TypeAnnotationContext::DeclarationTarget => (
            "Variable Declaration",
            "Use a normal type name until traits are implemented",
        ),
        TypeAnnotationContext::SignatureParameter => (
            "Parameter Type Parsing",
            "Use a normal parameter or field type name until traits are implemented",
        ),
        TypeAnnotationContext::SignatureReturn => (
            "Function Signature Parsing",
            "Use a normal return type until traits are implemented",
        ),
    }
}

fn expected_type_error(
    context: TypeAnnotationContext,
) -> (&'static str, &'static str, &'static str) {
    match context {
        TypeAnnotationContext::DeclarationTarget => (
            "Invalid token after declaration name. Expected a type or assignment operator.",
            "Variable Declaration",
            "Use a type declaration (Int, String, etc.) or assignment operator '='",
        ),
        TypeAnnotationContext::SignatureParameter => (
            "Expected a parameter type declaration",
            "Parameter Type Parsing",
            "Add a type declaration (Int, String, Float, Bool, a struct name, or a collection type) after the parameter name",
        ),
        TypeAnnotationContext::SignatureReturn => (
            "Expected a concrete return type",
            "Function Signature Parsing",
            "Use a supported return type such as Int, String, Float, Bool, a struct name, or a collection type",
        ),
    }
}

fn compilation_stage(context: TypeAnnotationContext) -> &'static str {
    match context {
        TypeAnnotationContext::DeclarationTarget => "Variable Declaration",
        TypeAnnotationContext::SignatureParameter => "Parameter Type Parsing",
        TypeAnnotationContext::SignatureReturn => "Function Signature Parsing",
    }
}

pub(crate) fn append_type_annotation_tokens(
    tokens: &mut Vec<Token>,
    annotation: &TypeAnnotationSyntax,
    location: &SourceLocation,
) {
    append_data_type_tokens(tokens, &annotation.data_type, location);
}

pub(crate) fn append_data_type_tokens(
    tokens: &mut Vec<Token>,
    data_type: &DataType,
    location: &SourceLocation,
) {
    match data_type {
        DataType::Inferred => {}
        DataType::Int => tokens.push(Token::new(TokenKind::DatatypeInt, location.clone())),
        DataType::Float => tokens.push(Token::new(TokenKind::DatatypeFloat, location.clone())),
        DataType::Bool => tokens.push(Token::new(TokenKind::DatatypeBool, location.clone())),
        DataType::StringSlice => {
            tokens.push(Token::new(TokenKind::DatatypeString, location.clone()))
        }
        DataType::Char => tokens.push(Token::new(TokenKind::DatatypeChar, location.clone())),
        DataType::NamedType(type_name) => {
            tokens.push(Token::new(TokenKind::Symbol(*type_name), location.clone()))
        }
        DataType::Collection(inner, _) => {
            tokens.push(Token::new(TokenKind::OpenCurly, location.clone()));
            append_data_type_tokens(tokens, inner.as_ref(), location);
            tokens.push(Token::new(TokenKind::CloseCurly, location.clone()));
        }
        DataType::Option(inner) => {
            append_data_type_tokens(tokens, inner.as_ref(), location);
            tokens.push(Token::new(TokenKind::QuestionMark, location.clone()));
        }
        _ => {}
    }
}

pub(crate) fn for_each_named_type_in_data_type(
    data_type: &DataType,
    visitor: &mut impl FnMut(StringId),
) {
    match data_type {
        DataType::NamedType(type_name) => visitor(*type_name),
        DataType::Collection(inner, _) | DataType::Option(inner) | DataType::Reference(inner) => {
            for_each_named_type_in_data_type(inner, visitor)
        }
        DataType::Returns(values) => {
            for value in values {
                for_each_named_type_in_data_type(value, visitor);
            }
        }
        _ => {}
    }
}

pub(crate) fn resolve_named_type(
    type_name: StringId,
    location: &SourceLocation,
    resolve_by_name: &mut impl FnMut(StringId) -> Option<DataType>,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    resolve_by_name(type_name).ok_or_else(|| {
        CompilerError::new_rule_error(
            format!(
                "Unknown type '{}'. Type names must be declared before use.",
                string_table.resolve(type_name)
            ),
            location.clone(),
        )
    })
}

pub(crate) fn resolve_named_types_in_data_type(
    data_type: &DataType,
    location: &SourceLocation,
    resolve_by_name: &mut impl FnMut(StringId) -> Option<DataType>,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    match data_type {
        DataType::NamedType(type_name) => {
            resolve_named_type(*type_name, location, resolve_by_name, string_table)
        }
        DataType::Collection(inner, ownership) => Ok(DataType::Collection(
            Box::new(resolve_named_types_in_data_type(
                inner,
                location,
                resolve_by_name,
                string_table,
            )?),
            ownership.to_owned(),
        )),
        DataType::Option(inner) => Ok(DataType::Option(Box::new(
            resolve_named_types_in_data_type(inner, location, resolve_by_name, string_table)?,
        ))),
        DataType::Reference(inner) => Ok(DataType::Reference(Box::new(
            resolve_named_types_in_data_type(inner, location, resolve_by_name, string_table)?,
        ))),
        DataType::Returns(values) => {
            let mut resolved_values = Vec::with_capacity(values.len());
            for value in values {
                resolved_values.push(resolve_named_types_in_data_type(
                    value,
                    location,
                    resolve_by_name,
                    string_table,
                )?);
            }
            Ok(DataType::Returns(resolved_values))
        }
        _ => Ok(data_type.to_owned()),
    }
}

#[cfg(test)]
#[path = "tests/type_syntax_tests.rs"]
mod type_syntax_tests;
