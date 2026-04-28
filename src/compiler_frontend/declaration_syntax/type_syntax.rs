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
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword_error, reserved_trait_keyword_or_dispatch_mismatch,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::return_syntax_error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeAnnotationContext {
    DeclarationTarget,
    SignatureParameter,
    SignatureReturn,
    TypeAliasTarget,
}

pub(crate) fn parse_type_annotation(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
) -> Result<DataType, CompilerError> {
    // Regular declarations can be inferred datatypes
    // So they can break out early with an Inferred type.
    if matches!(context, TypeAnnotationContext::DeclarationTarget)
        && matches!(
            token_stream.current_token_kind(),
            TokenKind::Assign | TokenKind::Newline | TokenKind::Comma
        )
    {
        return Ok(DataType::Inferred);
    }

    // Otherwise, parse the type that must be there
    let parsed_type = parse_required_type(token_stream, context)?;
    Ok(parsed_type)
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
            let keyword = reserved_trait_keyword_or_dispatch_mismatch(
                token_stream.current_token_kind(),
                token_stream.current_location(),
                compilation_stage(context),
                "type annotation parsing",
            )?;

            let (stage, suggestion) = reserved_trait_type_annotation_error(context);
            return Err(reserved_trait_keyword_error(
                keyword,
                token_stream.current_location(),
                stage,
                suggestion,
            ));
        }
        TokenKind::OpenCurly => parse_collection_type(token_stream, context)?,
        TokenKind::As => {
            let stage = compilation_stage(context);
            return_syntax_error!(
                "`as` is not valid here. It is only supported in type aliases, import clauses, and choice payload patterns.",
                token_stream.current_location(),
                {
                    CompilationStage => stage,
                    PrimarySuggestion => "Use a type name or remove `as`",
                }
            );
        }
        TokenKind::Symbol(type_name) => {
            let type_name = *type_name;
            token_stream.advance();
            DataType::NamedType(type_name)
        }
        TokenKind::Colon if matches!(context, TypeAnnotationContext::DeclarationTarget) => {
            return_syntax_error!(
                "Unexpected ':' after declaration name. Beanstalk does not support bare labeled blocks or `name: Type` declarations. Use `block:` for a scoped block, or write declarations as `name Type = value`.",
                token_stream.current_location(),
                {
                    CompilationStage => "Variable Declaration",
                    PrimarySuggestion => "Use `block:` for a scoped block, or write declarations as `name Type = value`.",
                }
            );
        }
        other
            if matches!(context, TypeAnnotationContext::DeclarationTarget)
                && matches!(
                    other,
                    TokenKind::Dot
                        | TokenKind::AddAssign
                        | TokenKind::SubtractAssign
                        | TokenKind::DivideAssign
                        | TokenKind::IntDivideAssign
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

    Ok(DataType::Collection(Box::new(inner_type)))
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
        TypeAnnotationContext::TypeAliasTarget => (
            "None is not a valid type alias target",
            "Type Alias Parsing",
            "Use a concrete type such as Int, String, Float, Bool, a struct name, or a collection type",
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
        TypeAnnotationContext::TypeAliasTarget => (
            "Type Alias Parsing",
            "Use a normal type name until traits are implemented",
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
        TypeAnnotationContext::TypeAliasTarget => (
            "Expected a type after `as` in type alias declaration.",
            "Type Alias Parsing",
            "Provide a valid type after `as`, e.g. `UserId as Int`",
        ),
    }
}

fn compilation_stage(context: TypeAnnotationContext) -> &'static str {
    match context {
        TypeAnnotationContext::DeclarationTarget => "Variable Declaration",
        TypeAnnotationContext::SignatureParameter => "Parameter Type Parsing",
        TypeAnnotationContext::SignatureReturn => "Function Signature Parsing",
        TypeAnnotationContext::TypeAliasTarget => "Type Alias Parsing",
    }
}

pub(crate) fn for_each_named_type_in_data_type(
    data_type: &DataType,
    visitor: &mut impl FnMut(StringId),
) {
    match data_type {
        DataType::NamedType(type_name) => visitor(*type_name),
        DataType::Collection(inner) | DataType::Option(inner) | DataType::Reference(inner) => {
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
        DataType::Collection(inner) => Ok(DataType::Collection(Box::new(
            resolve_named_types_in_data_type(inner, location, resolve_by_name, string_table)?,
        ))),
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
        DataType::Choices {
            nominal_path,
            variants,
        } => {
            use crate::compiler_frontend::declaration_syntax::choice::{
                ChoiceVariant, ChoiceVariantPayload,
            };
            let mut resolved_variants = Vec::with_capacity(variants.len());
            for variant in variants {
                let payload = match &variant.payload {
                    ChoiceVariantPayload::Unit => ChoiceVariantPayload::Unit,
                    ChoiceVariantPayload::Record { fields } => {
                        let mut resolved_fields = Vec::with_capacity(fields.len());
                        for field in fields {
                            let mut resolved_field = field.to_owned();
                            resolved_field.value.data_type = resolve_named_types_in_data_type(
                                &field.value.data_type,
                                &field.value.location,
                                resolve_by_name,
                                string_table,
                            )?;
                            resolved_fields.push(resolved_field);
                        }
                        ChoiceVariantPayload::Record {
                            fields: resolved_fields,
                        }
                    }
                };
                resolved_variants.push(ChoiceVariant {
                    id: variant.id,
                    payload,
                    location: variant.location.clone(),
                });
            }
            Ok(DataType::Choices {
                nominal_path: nominal_path.to_owned(),
                variants: resolved_variants,
            })
        }
        _ => Ok(data_type.to_owned()),
    }
}

#[cfg(test)]
#[path = "tests/type_syntax_tests.rs"]
mod type_syntax_tests;
