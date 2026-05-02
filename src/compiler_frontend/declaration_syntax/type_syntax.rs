//! Shared frontend type-annotation syntax and named-type resolution helpers.
//!
//! WHAT: owns parsing/serialization of explicit type annotations and recursive
//! resolution of frontend type placeholders.
//! WHY: declaration parsing, signature parsing, and AST type-resolution all
//! used to maintain parallel implementations that drifted in diagnostics and
//! behavior.
//!
//! This module owns:
//! - token-to-type annotation parsing for declaration/signature contexts
//! - optional suffix (`?`) annotation rules
//! - recursive type resolution with consistent unknown-type diagnostics
//! - annotation token emission helpers used by header/declaration plumbing
//!
//! This module does NOT own:
//! - declaration/statement-level semantics (mutability rules, initializer rules)
//! - expression typing/coercion policy
//! - call-site/feature-specific diagnostic framing outside type syntax itself

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::statements::functions::FunctionReturn;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::generics::{
    BuiltinGenericType, GenericBaseType, GenericInstantiationKey, GenericParameterScope,
    TypeIdentityKey, TypeSubstitution, data_type_to_type_identity_key,
};
use crate::compiler_frontend::declaration_syntax::choice::{ChoiceVariant, ChoiceVariantPayload};
use crate::compiler_frontend::deferred_feature_diagnostics::deferred_feature_rule_error;
use crate::compiler_frontend::external_packages::ExternalSymbolId;
use crate::compiler_frontend::headers::module_symbols::{
    GenericDeclarationKind, GenericDeclarationMetadata,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword_error, reserved_trait_keyword_or_dispatch_mismatch,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::return_syntax_error;
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeAnnotationContext {
    DeclarationTarget,
    SignatureParameter,
    SignatureReturn,
    TypeAliasTarget,
}

/// Collection capacity parsed from a collection type annotation such as `{Int 64}`.
///
/// WHAT: capacity is allocation metadata, not part of type identity.
/// WHY: keeps capacity separate from the generic type model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CollectionCapacity {
    pub value: i64,
    pub location: SourceLocation,
}

/// Result of parsing a type annotation, including optional collection capacity.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParsedTypeAnnotation {
    pub data_type: DataType,
    pub collection_capacity: Option<CollectionCapacity>,
}

impl ParsedTypeAnnotation {
    pub fn new(data_type: DataType) -> Self {
        Self {
            data_type,
            collection_capacity: None,
        }
    }
}

pub(crate) fn parse_type_annotation(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
) -> Result<DataType, CompilerError> {
    parse_type_annotation_with_capacity(token_stream, context).map(|parsed| parsed.data_type)
}

pub(crate) fn parse_type_annotation_with_capacity(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
) -> Result<ParsedTypeAnnotation, CompilerError> {
    // Regular declarations can be inferred datatypes
    // So they can break out early with an Inferred type.
    if matches!(context, TypeAnnotationContext::DeclarationTarget)
        && matches!(
            token_stream.current_token_kind(),
            TokenKind::Assign | TokenKind::Newline | TokenKind::Comma
        )
    {
        return Ok(ParsedTypeAnnotation::new(DataType::Inferred));
    }

    parse_required_type(token_stream, context)
}

fn parse_required_type(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
) -> Result<ParsedTypeAnnotation, CompilerError> {
    parse_required_type_with_generic_application(token_stream, context, true)
}

fn parse_required_type_with_generic_application(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
    allow_generic_application: bool,
) -> Result<ParsedTypeAnnotation, CompilerError> {
    let parsed_atom = parse_type_atom(token_stream, context)?;
    parse_type_postfixes(
        token_stream,
        parsed_atom,
        context,
        allow_generic_application,
    )
}

fn parse_type_atom(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
) -> Result<ParsedTypeAnnotation, CompilerError> {
    match token_stream.current_token_kind() {
        TokenKind::DatatypeInt => {
            token_stream.advance();
            Ok(ParsedTypeAnnotation::new(DataType::Int))
        }
        TokenKind::DatatypeFloat => {
            token_stream.advance();
            Ok(ParsedTypeAnnotation::new(DataType::Float))
        }
        TokenKind::DatatypeBool => {
            token_stream.advance();
            Ok(ParsedTypeAnnotation::new(DataType::Bool))
        }
        TokenKind::DatatypeString => {
            token_stream.advance();
            Ok(ParsedTypeAnnotation::new(DataType::StringSlice))
        }
        TokenKind::DatatypeChar => {
            token_stream.advance();
            Ok(ParsedTypeAnnotation::new(DataType::Char))
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
            Err(reserved_trait_keyword_error(
                keyword,
                token_stream.current_location(),
                stage,
                suggestion,
            ))
        }
        TokenKind::OpenCurly => parse_collection_type(token_stream, context),
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
        TokenKind::Type => Err(type_keyword_deferred_error(token_stream, context)),
        TokenKind::Of => Err(of_keyword_syntax_error(token_stream, context)),
        TokenKind::Symbol(type_name) => {
            let type_name = *type_name;
            token_stream.advance();
            Ok(ParsedTypeAnnotation::new(DataType::NamedType(type_name)))
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
    }
}

fn parse_type_postfixes(
    token_stream: &mut FileTokens,
    parsed_type: ParsedTypeAnnotation,
    context: TypeAnnotationContext,
    allow_generic_application: bool,
) -> Result<ParsedTypeAnnotation, CompilerError> {
    let with_generic_arguments = parse_generic_arguments(
        token_stream,
        parsed_type,
        context,
        allow_generic_application,
    )?;
    parse_optional_type_suffix(token_stream, with_generic_arguments, context)
}

fn parse_collection_type(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
) -> Result<ParsedTypeAnnotation, CompilerError> {
    token_stream.advance();

    let inner = if token_stream.current_token_kind() == &TokenKind::CloseCurly {
        ParsedTypeAnnotation::new(DataType::Inferred)
    } else {
        parse_required_type_with_generic_application(token_stream, context, true)?
    };

    // Check for optional capacity after the element type.
    let capacity = if let TokenKind::IntLiteral(value) = token_stream.current_token_kind() {
        let capacity_location = token_stream.current_location();
        if *value < 0 {
            return_syntax_error!(
                "Collection capacity must be a non-negative integer.",
                capacity_location,
                {
                    CompilationStage => compilation_stage(context),
                    PrimarySuggestion => "Use a positive integer or zero for collection capacity.",
                }
            );
        }
        let cap = CollectionCapacity {
            value: *value,
            location: capacity_location,
        };
        token_stream.advance();
        Some(cap)
    } else {
        None
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

    let data_type = DataType::collection(inner.data_type);
    Ok(ParsedTypeAnnotation {
        data_type,
        collection_capacity: capacity.or(inner.collection_capacity),
    })
}

fn parse_generic_arguments(
    token_stream: &mut FileTokens,
    parsed_type: ParsedTypeAnnotation,
    context: TypeAnnotationContext,
    allow_generic_application: bool,
) -> Result<ParsedTypeAnnotation, CompilerError> {
    if token_stream.current_token_kind() != &TokenKind::Of {
        return Ok(parsed_type);
    }

    if !allow_generic_application {
        return Err(nested_generic_application_error(
            token_stream.current_location(),
            context,
        ));
    }

    let base = match parsed_type.data_type {
        DataType::NamedType(type_name) => GenericBaseType::Named(type_name),
        _ => {
            return_syntax_error!(
                "`of` can only apply generic arguments to a named type.",
                token_stream.current_location(),
                {
                    CompilationStage => compilation_stage(context),
                    PrimarySuggestion => "Write generic applications as `TypeName of ArgumentType`",
                }
            )
        }
    };

    token_stream.advance();

    let mut arguments = Vec::new();
    loop {
        if generic_argument_list_is_finished(token_stream.current_token_kind()) {
            if arguments.is_empty() {
                return_syntax_error!(
                    "Expected at least one type argument after `of`.",
                    token_stream.current_location(),
                    {
                        CompilationStage => compilation_stage(context),
                        PrimarySuggestion => "Add a concrete type argument after `of`",
                    }
                );
            }
            break;
        }

        arguments.push(parse_generic_type_argument(token_stream, context)?);

        match token_stream.current_token_kind() {
            TokenKind::Comma => {
                token_stream.advance();
                if generic_argument_list_is_finished(token_stream.current_token_kind()) {
                    return_syntax_error!(
                        "Expected a type argument after ','.",
                        token_stream.current_location(),
                        {
                            CompilationStage => compilation_stage(context),
                            PrimarySuggestion => "Remove the trailing comma or add another type argument",
                        }
                    );
                }
            }
            token if generic_argument_list_is_finished(token) => break,
            TokenKind::Of => {
                return Err(nested_generic_application_error(
                    token_stream.current_location(),
                    context,
                ));
            }
            other => {
                return_syntax_error!(
                    format!(
                        "Expected ',' or the end of this type annotation after generic argument, found '{other:?}'.",
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => compilation_stage(context),
                        PrimarySuggestion => "Separate generic type arguments with commas",
                    }
                )
            }
        }
    }

    Ok(ParsedTypeAnnotation {
        data_type: DataType::GenericInstance { base, arguments },
        collection_capacity: parsed_type.collection_capacity,
    })
}

fn parse_generic_type_argument(
    token_stream: &mut FileTokens,
    context: TypeAnnotationContext,
) -> Result<DataType, CompilerError> {
    let parsed_argument = parse_type_atom(token_stream, context)?;

    if token_stream.current_token_kind() == &TokenKind::Of {
        return Err(nested_generic_application_error(
            token_stream.current_location(),
            context,
        ));
    }

    Ok(parsed_argument.data_type)
}

fn generic_argument_list_is_finished(token: &TokenKind) -> bool {
    matches!(
        token,
        TokenKind::Assign
            | TokenKind::Newline
            | TokenKind::Colon
            | TokenKind::TypeParameterBracket
            | TokenKind::CloseCurly
            | TokenKind::Bang
            | TokenKind::QuestionMark
            | TokenKind::Eof
            | TokenKind::End
            | TokenKind::IntLiteral(_)
    )
}

fn nested_generic_application_error(
    location: SourceLocation,
    context: TypeAnnotationContext,
) -> CompilerError {
    deferred_feature_rule_error(
        "Nested generic type applications are not supported in a single annotation.",
        location,
        compilation_stage(context),
        "Name the inner type with a concrete type alias first.",
    )
}

fn parse_optional_type_suffix(
    token_stream: &mut FileTokens,
    parsed_type: ParsedTypeAnnotation,
    context: TypeAnnotationContext,
) -> Result<ParsedTypeAnnotation, CompilerError> {
    if token_stream.current_token_kind() != &TokenKind::QuestionMark {
        return Ok(parsed_type);
    }

    if matches!(parsed_type.data_type, DataType::Option(_)) {
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

    Ok(ParsedTypeAnnotation {
        data_type: DataType::Option(Box::new(parsed_type.data_type)),
        collection_capacity: parsed_type.collection_capacity,
    })
}

fn type_keyword_deferred_error(
    token_stream: &FileTokens,
    context: TypeAnnotationContext,
) -> CompilerError {
    deferred_feature_rule_error(
        "`type` starts a generic declaration and is not valid inside a type annotation.",
        token_stream.current_location(),
        compilation_stage(context),
        "Use `type` after a top-level declaration name, for example `Box type T = | ... |`.",
    )
}

fn of_keyword_syntax_error(
    token_stream: &FileTokens,
    context: TypeAnnotationContext,
) -> CompilerError {
    let (message, stage, suggestion) = of_keyword_error(context);
    let mut error = CompilerError::new_syntax_error(message, token_stream.current_location());
    error.new_metadata_entry(
        crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::CompilationStage,
        stage.to_owned(),
    );
    error.new_metadata_entry(
        crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
        suggestion.to_owned(),
    );
    error
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

fn of_keyword_error(context: TypeAnnotationContext) -> (&'static str, &'static str, &'static str) {
    match context {
        TypeAnnotationContext::DeclarationTarget => (
            "Unexpected `of` in declaration type position.",
            "Variable Declaration",
            "Write generic applications after a base type, for example `Box of String`.",
        ),
        TypeAnnotationContext::SignatureParameter => (
            "Unexpected `of` in parameter type position.",
            "Parameter Type Parsing",
            "Write generic applications after a base type, for example `Box of String`.",
        ),
        TypeAnnotationContext::SignatureReturn => (
            "Unexpected `of` in return type position.",
            "Function Signature Parsing",
            "Write generic applications after a base type, for example `Box of String`.",
        ),
        TypeAnnotationContext::TypeAliasTarget => (
            "Unexpected `of` in type alias target.",
            "Type Alias Parsing",
            "Write generic applications after a base type, for example `Box of String`.",
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

pub(crate) struct TypeResolutionContext<'a> {
    pub declarations: &'a [Declaration],
    pub visible_declaration_ids: Option<&'a FxHashSet<InternedPath>>,
    pub visible_external_symbols: Option<&'a FxHashMap<StringId, ExternalSymbolId>>,
    pub visible_source_bindings: Option<&'a FxHashMap<StringId, InternedPath>>,
    pub visible_type_aliases: Option<&'a FxHashMap<StringId, InternedPath>>,
    pub resolved_type_aliases: Option<&'a FxHashMap<InternedPath, DataType>>,
    pub generic_declarations_by_path:
        Option<&'a FxHashMap<InternedPath, GenericDeclarationMetadata>>,
    pub generic_parameters: Option<&'a GenericParameterScope>,
    /// Resolved struct fields by canonical path, including generic struct templates.
    /// Required for lazy generic struct instantiation.
    pub resolved_struct_fields_by_path: Option<&'a FxHashMap<InternedPath, Vec<Declaration>>>,
    /// Mutable cache for lazily instantiated generic nominal types.
    pub generic_nominal_instantiations:
        Option<&'a std::cell::RefCell<FxHashMap<GenericInstantiationKey, DataType>>>,
}

impl<'a> TypeResolutionContext<'a> {
    #[allow(dead_code)] // Used by tests and planned call sites while Phase 0 wiring lands.
    pub(crate) fn from_declarations(declarations: &'a [Declaration]) -> Self {
        Self {
            declarations,
            visible_declaration_ids: None,
            visible_external_symbols: None,
            visible_source_bindings: None,
            visible_type_aliases: None,
            resolved_type_aliases: None,
            generic_declarations_by_path: None,
            generic_parameters: None,
            resolved_struct_fields_by_path: None,
            generic_nominal_instantiations: None,
        }
    }
}

pub(crate) fn for_each_named_type_in_data_type(
    data_type: &DataType,
    visitor: &mut impl FnMut(StringId),
) {
    match data_type {
        DataType::NamedType(type_name) => visitor(*type_name),
        DataType::GenericInstance { base, arguments } => {
            if let GenericBaseType::Named(name) = base {
                visitor(*name);
            }
            for argument in arguments {
                for_each_named_type_in_data_type(argument, visitor);
            }
        }
        DataType::Option(inner) | DataType::Reference(inner) => {
            for_each_named_type_in_data_type(inner, visitor)
        }
        DataType::Result { ok, err } => {
            for_each_named_type_in_data_type(ok, visitor);
            for_each_named_type_in_data_type(err, visitor);
        }
        DataType::Returns(values) => {
            for value in values {
                for_each_named_type_in_data_type(value, visitor);
            }
        }
        DataType::Function(_, signature) => {
            for parameter in &signature.parameters {
                for_each_named_type_in_data_type(&parameter.value.data_type, visitor);
            }
            for return_slot in &signature.returns {
                for_each_named_type_in_data_type(return_slot.data_type(), visitor);
            }
        }
        DataType::Struct { fields, .. } | DataType::Parameters(fields) => {
            for field in fields {
                for_each_named_type_in_data_type(&field.value.data_type, visitor);
            }
        }
        DataType::Choices { variants, .. } => {
            for variant in variants {
                if let ChoiceVariantPayload::Record { fields } = &variant.payload {
                    for field in fields {
                        for_each_named_type_in_data_type(&field.value.data_type, visitor);
                    }
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn resolve_type(
    data_type: &DataType,
    location: &SourceLocation,
    context: &TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    match data_type {
        DataType::NamedType(type_name) => {
            resolve_named_type_from_context(*type_name, location, context, string_table)
        }
        DataType::TypeParameter { .. } => Ok(data_type.to_owned()),
        DataType::GenericInstance { base, arguments } => {
            let resolved_base =
                resolve_generic_base_type(base, arguments, location, context, string_table)?;
            let mut resolved_arguments = Vec::with_capacity(arguments.len());
            for argument in arguments {
                resolved_arguments.push(resolve_type(argument, location, context, string_table)?);
            }

            // Attempt lazy instantiation for user-declared generic structs/choices.
            if let GenericBaseType::ResolvedNominal(base_path) = &resolved_base
                && let Some(metadata) = context
                    .generic_declarations_by_path
                    .and_then(|decls| decls.get(base_path))
                && let Some(instantiated) = instantiate_generic_nominal(
                    base_path,
                    metadata,
                    &resolved_arguments,
                    location,
                    context,
                    string_table,
                )?
            {
                return Ok(instantiated);
            }

            Ok(DataType::GenericInstance {
                base: resolved_base,
                arguments: resolved_arguments,
            })
        }
        DataType::Option(inner) => Ok(DataType::Option(Box::new(resolve_type(
            inner,
            location,
            context,
            string_table,
        )?))),
        DataType::Reference(inner) => Ok(DataType::Reference(Box::new(resolve_type(
            inner,
            location,
            context,
            string_table,
        )?))),
        DataType::Returns(values) => {
            let mut resolved_values = Vec::with_capacity(values.len());
            for value in values {
                resolved_values.push(resolve_type(value, location, context, string_table)?);
            }
            Ok(DataType::Returns(resolved_values))
        }
        DataType::Result { ok, err } => Ok(DataType::Result {
            ok: Box::new(resolve_type(ok, location, context, string_table)?),
            err: Box::new(resolve_type(err, location, context, string_table)?),
        }),
        DataType::Function(receiver, signature) => {
            let resolved_receiver = receiver
                .as_ref()
                .as_ref()
                .map(|receiver_key| receiver_key.to_owned());

            let mut resolved_signature = signature.to_owned();
            for parameter in &mut resolved_signature.parameters {
                parameter.value.data_type = resolve_type(
                    &parameter.value.data_type,
                    &parameter.value.location,
                    context,
                    string_table,
                )?;
            }

            for return_slot in &mut resolved_signature.returns {
                match &mut return_slot.value {
                    FunctionReturn::Value(return_type) => {
                        *return_type = resolve_type(return_type, location, context, string_table)?;
                    }
                    FunctionReturn::AliasCandidates { data_type, .. } => {
                        *data_type = resolve_type(data_type, location, context, string_table)?;
                    }
                }
            }

            Ok(DataType::Function(
                Box::new(resolved_receiver),
                resolved_signature,
            ))
        }
        DataType::Struct {
            nominal_path,
            fields,
            const_record,
            ..
        } => {
            let mut resolved_fields = Vec::with_capacity(fields.len());
            for field in fields {
                let mut resolved_field = field.to_owned();
                resolved_field.value.data_type = resolve_type(
                    &field.value.data_type,
                    &field.value.location,
                    context,
                    string_table,
                )?;
                resolved_fields.push(resolved_field);
            }

            Ok(DataType::Struct {
                nominal_path: nominal_path.to_owned(),
                fields: resolved_fields,
                const_record: *const_record,
                generic_instance_key: None,
            })
        }
        DataType::Choices {
            nominal_path,
            variants,
            ..
        } => {
            let mut resolved_variants = Vec::with_capacity(variants.len());
            for variant in variants {
                resolved_variants.push(resolve_choice_variant_types(
                    variant,
                    context,
                    string_table,
                )?);
            }

            Ok(DataType::Choices {
                nominal_path: nominal_path.to_owned(),
                variants: resolved_variants,
                generic_instance_key: None,
            })
        }
        DataType::Parameters(parameters) => {
            let mut resolved_parameters = Vec::with_capacity(parameters.len());
            for parameter in parameters {
                let mut resolved_parameter = parameter.to_owned();
                resolved_parameter.value.data_type = resolve_type(
                    &parameter.value.data_type,
                    &parameter.value.location,
                    context,
                    string_table,
                )?;
                resolved_parameters.push(resolved_parameter);
            }

            Ok(DataType::Parameters(resolved_parameters))
        }
        _ => Ok(data_type.to_owned()),
    }
}

fn resolve_choice_variant_types(
    variant: &ChoiceVariant,
    context: &TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> Result<ChoiceVariant, CompilerError> {
    let payload = match &variant.payload {
        ChoiceVariantPayload::Unit => ChoiceVariantPayload::Unit,
        ChoiceVariantPayload::Record { fields } => {
            let mut resolved_fields = Vec::with_capacity(fields.len());
            for field in fields {
                let mut resolved_field = field.to_owned();
                resolved_field.value.data_type = resolve_type(
                    &field.value.data_type,
                    &field.value.location,
                    context,
                    string_table,
                )?;
                resolved_fields.push(resolved_field);
            }
            ChoiceVariantPayload::Record {
                fields: resolved_fields,
            }
        }
    };

    Ok(ChoiceVariant {
        id: variant.id,
        payload,
        location: variant.location.to_owned(),
    })
}

/// Lazily instantiate a generic struct or choice declaration with concrete type arguments.
///
/// WHAT: looks up the template fields/variants, substitutes type parameters, and caches
///       the concrete nominal type.
/// WHY: generic structs/choices must be fully concrete before HIR lowering.
///
/// Returns `Ok(Some(DataType))` on successful instantiation, `Ok(None)` when template data
/// is not available (call site should fall back to GenericInstance), or `Err` on failure.
fn instantiate_generic_nominal(
    base_path: &InternedPath,
    metadata: &GenericDeclarationMetadata,
    arguments: &[DataType],
    location: &SourceLocation,
    context: &TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> Result<Option<DataType>, CompilerError> {
    let param_count = metadata.parameters.len();
    if arguments.len() != param_count {
        return Err(CompilerError::new_rule_error(
            format!(
                "Generic type '{}' expects {} type argument(s), but {} were provided.",
                base_path.to_string(string_table),
                param_count,
                arguments.len()
            ),
            location.to_owned(),
        ));
    }

    // Build argument identity keys. If any argument can't be keyed (e.g. TypeParameter
    // inside an unresolved generic function body), we still substitute but skip caching.
    let arg_keys: Vec<TypeIdentityKey> = arguments
        .iter()
        .filter_map(data_type_to_type_identity_key)
        .collect();
    let all_concrete = arg_keys.len() == arguments.len();

    let key = GenericInstantiationKey {
        base_path: base_path.to_owned(),
        arguments: arg_keys,
    };

    // Check cache first.
    if all_concrete
        && let Some(cache) = context.generic_nominal_instantiations
        && let Some(cached) = cache.borrow().get(&key).cloned()
    {
        return Ok(Some(cached));
    }

    // Build substitution mapping parameter ids -> concrete arguments.
    let mut substitution = TypeSubstitution::empty();
    for (param, arg) in metadata.parameters.parameters.iter().zip(arguments.iter()) {
        substitution.insert(param.id, arg.to_owned());
    }

    let instantiated = match metadata.kind {
        GenericDeclarationKind::Struct => {
            let Some(fields_map) = context.resolved_struct_fields_by_path else {
                // Template data unavailable; caller should fall back to GenericInstance.
                return Ok(None);
            };
            let Some(template_fields) = fields_map.get(base_path) else {
                // Template not yet available (e.g. recursive generic type during its own
                // resolution). Fall back to GenericInstance so the caller can reject it
                // with a proper recursive-type diagnostic.
                return Ok(None);
            };

            let substituted_fields = template_fields
                .iter()
                .map(|field| {
                    let mut resolved = field.to_owned();
                    resolved.value.data_type =
                        crate::compiler_frontend::datatypes::generics::substitute_type_parameters(
                            &field.value.data_type,
                            &substitution,
                        );
                    resolved
                })
                .collect();

            DataType::Struct {
                nominal_path: base_path.to_owned(),
                fields: substituted_fields,
                const_record: false,
                generic_instance_key: if all_concrete {
                    Some(key.to_owned())
                } else {
                    None
                },
            }
        }
        GenericDeclarationKind::Choice => {
            let template_declaration = context.declarations.iter().rfind(|declaration| {
                &declaration.id == base_path
                    && matches!(declaration.value.data_type, DataType::Choices { .. })
            });
            let Some(template_declaration) = template_declaration else {
                // Template not yet available (e.g. recursive generic type during its own
                // resolution). Fall back to GenericInstance so the caller can reject it
                // with a proper recursive-type diagnostic.
                return Ok(None);
            };
            let DataType::Choices {
                variants: template_variants,
                ..
            } = &template_declaration.value.data_type
            else {
                unreachable!("Template declaration filter guarantees Choices variant");
            };

            let substituted_variants = template_variants
                .iter()
                .map(|variant| {
                    let payload = match &variant.payload {
                        ChoiceVariantPayload::Unit => ChoiceVariantPayload::Unit,
                        ChoiceVariantPayload::Record { fields } => {
                            let substituted_fields = fields
                                .iter()
                                .map(|field| {
                                    let mut resolved = field.to_owned();
                                    resolved.value.data_type = crate::compiler_frontend::datatypes::generics::substitute_type_parameters(
                                        &field.value.data_type,
                                        &substitution,
                                    );
                                    resolved
                                })
                                .collect();
                            ChoiceVariantPayload::Record {
                                fields: substituted_fields,
                            }
                        }
                    };
                    ChoiceVariant {
                        id: variant.id,
                        payload,
                        location: variant.location.to_owned(),
                    }
                })
                .collect();

            DataType::Choices {
                nominal_path: base_path.to_owned(),
                variants: substituted_variants,
                generic_instance_key: if all_concrete {
                    Some(key.to_owned())
                } else {
                    None
                },
            }
        }
        _ => {
            // Not a generic struct or choice; fall back to GenericInstance.
            return Ok(None);
        }
    };

    if all_concrete && let Some(cache) = context.generic_nominal_instantiations {
        cache.borrow_mut().insert(key, instantiated.clone());
    }

    Ok(Some(instantiated))
}

fn resolve_named_type_from_context(
    type_name: StringId,
    location: &SourceLocation,
    context: &TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    // 1) Generic parameter scope.
    if let Some(generic_scope) = context.generic_parameters
        && let Some(parameter) = generic_scope.resolve(type_name)
    {
        return Ok(DataType::TypeParameter {
            id: parameter.id,
            name: parameter.name,
        });
    }

    // 2) Visible type aliases.
    if let Some(visible_aliases) = context.visible_type_aliases
        && let Some(alias_path) = visible_aliases.get(&type_name)
        && let Some(resolved_aliases) = context.resolved_type_aliases
        && let Some(resolved) = resolved_aliases.get(alias_path)
    {
        return Ok(resolved.to_owned());
    }

    // 3) Visible source declarations (path-based first, then name fallback).
    if let Some(visible_source_bindings) = context.visible_source_bindings
        && let Some(canonical_path) = visible_source_bindings.get(&type_name)
        && let Some(declaration) = resolve_declaration_by_path(
            context.declarations,
            context.visible_declaration_ids,
            canonical_path,
        )
    {
        reject_bare_generic_type_name(type_name, canonical_path, location, context, string_table)?;
        return Ok(declaration.value.data_type.to_owned());
    }

    if let Some(declaration) = visible_declaration_by_name(
        context.declarations,
        context.visible_declaration_ids,
        type_name,
    ) {
        reject_bare_generic_type_name(type_name, &declaration.id, location, context, string_table)?;
        return Ok(declaration.value.data_type.to_owned());
    }

    // 4) Visible external types.
    if let Some(external_symbols) = context.visible_external_symbols
        && let Some(ExternalSymbolId::Type(type_id)) = external_symbols.get(&type_name)
    {
        return Ok(DataType::External { type_id: *type_id });
    }

    // 5) Builtin type names that may still appear as named placeholders.
    if let Some(builtin_type) = builtin_named_type(type_name, string_table) {
        return Ok(builtin_type);
    }

    Err(CompilerError::new_rule_error(
        format!(
            "Unknown type '{}'. Type names must be declared before use.",
            string_table.resolve(type_name)
        ),
        location.to_owned(),
    ))
}

fn resolve_generic_base_type(
    base: &GenericBaseType,
    arguments: &[DataType],
    location: &SourceLocation,
    context: &TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> Result<GenericBaseType, CompilerError> {
    match base {
        GenericBaseType::Named(type_name) => {
            if let Some(generic_scope) = context.generic_parameters
                && generic_scope.contains_name(*type_name)
            {
                return Err(CompilerError::new_rule_error(
                    format!(
                        "Generic parameter '{}' cannot be used as a generic type constructor.",
                        string_table.resolve(*type_name)
                    ),
                    location.to_owned(),
                ));
            }

            if let Some(visible_source_bindings) = context.visible_source_bindings
                && let Some(canonical_path) = visible_source_bindings.get(type_name)
            {
                return resolve_generic_base_path(
                    Some(*type_name),
                    canonical_path,
                    arguments,
                    location,
                    context,
                    string_table,
                );
            }

            if let Some(declaration) = visible_declaration_by_name(
                context.declarations,
                context.visible_declaration_ids,
                *type_name,
            ) {
                return resolve_generic_base_path(
                    Some(*type_name),
                    &declaration.id,
                    arguments,
                    location,
                    context,
                    string_table,
                );
            }

            if let Some(visible_aliases) = context.visible_type_aliases
                && visible_aliases.contains_key(type_name)
            {
                return Err(CompilerError::new_rule_error(
                    format!(
                        "Type alias '{}' cannot be used as a generic type constructor.",
                        string_table.resolve(*type_name)
                    ),
                    location.to_owned(),
                ));
            }

            if let Some(external_symbols) = context.visible_external_symbols
                && matches!(
                    external_symbols.get(type_name),
                    Some(ExternalSymbolId::Type(_))
                )
            {
                return Err(CompilerError::new_rule_error(
                    format!(
                        "External type '{}' does not accept generic arguments.",
                        string_table.resolve(*type_name)
                    ),
                    location.to_owned(),
                ));
            }

            if builtin_named_type(*type_name, string_table).is_some() {
                return Err(CompilerError::new_rule_error(
                    format!(
                        "Builtin type '{}' does not accept generic arguments.",
                        string_table.resolve(*type_name)
                    ),
                    location.to_owned(),
                ));
            }

            Err(CompilerError::new_rule_error(
                format!(
                    "Unknown generic type '{}'. Generic type names must be declared before use.",
                    string_table.resolve(*type_name)
                ),
                location.to_owned(),
            ))
        }
        GenericBaseType::ResolvedNominal(path) => resolve_generic_base_path(
            path.name(),
            path,
            arguments,
            location,
            context,
            string_table,
        ),
        GenericBaseType::External(type_id) => Err(CompilerError::new_rule_error(
            format!(
                "External type '{}' does not accept generic arguments.",
                type_id.0
            ),
            location.to_owned(),
        )),
        GenericBaseType::Builtin(BuiltinGenericType::Collection) => {
            // Collection is the only builtin generic type allowed in source.
            // Its arguments are resolved separately by resolve_type.
            Ok(GenericBaseType::Builtin(BuiltinGenericType::Collection))
        }
    }
}

fn resolve_generic_base_path(
    visible_name: Option<StringId>,
    canonical_path: &InternedPath,
    arguments: &[DataType],
    location: &SourceLocation,
    context: &TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> Result<GenericBaseType, CompilerError> {
    let display_name = visible_name
        .map(|name| string_table.resolve(name).to_owned())
        .unwrap_or_else(|| canonical_path.to_string(string_table));

    let Some(metadata) = context
        .generic_declarations_by_path
        .and_then(|generic_declarations| generic_declarations.get(canonical_path))
    else {
        return Err(CompilerError::new_rule_error(
            format!("Type '{}' does not accept generic arguments.", display_name),
            location.to_owned(),
        ));
    };

    if !matches!(
        metadata.kind,
        GenericDeclarationKind::Struct | GenericDeclarationKind::Choice
    ) {
        return Err(CompilerError::new_rule_error(
            format!("'{}' is not a generic type declaration.", display_name),
            location.to_owned(),
        ));
    }

    let expected = metadata.parameters.len();
    let actual = arguments.len();
    if actual != expected {
        return Err(CompilerError::new_rule_error(
            format!(
                "Generic type '{}' expects {expected} type argument(s), but {actual} were provided.",
                display_name
            ),
            location.to_owned(),
        ));
    }

    Ok(GenericBaseType::ResolvedNominal(canonical_path.to_owned()))
}

fn reject_bare_generic_type_name(
    visible_name: StringId,
    canonical_path: &InternedPath,
    location: &SourceLocation,
    context: &TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    let Some(metadata) = context
        .generic_declarations_by_path
        .and_then(|generic_declarations| generic_declarations.get(canonical_path))
    else {
        return Ok(());
    };

    if matches!(
        metadata.kind,
        GenericDeclarationKind::Struct | GenericDeclarationKind::Choice
    ) {
        return Err(CompilerError::new_rule_error(
            format!(
                "Generic type '{}' requires type arguments.",
                string_table.resolve(visible_name)
            ),
            location.to_owned(),
        ));
    }

    Ok(())
}

fn resolve_declaration_by_path<'a>(
    declarations: &'a [Declaration],
    visible_declaration_ids: Option<&FxHashSet<InternedPath>>,
    canonical_path: &InternedPath,
) -> Option<&'a Declaration> {
    declarations.iter().rfind(|declaration| {
        &declaration.id == canonical_path
            && !declaration.is_unresolved_constant_placeholder()
            && match visible_declaration_ids {
                Some(visible) => visible.contains(&declaration.id),
                None => true,
            }
    })
}

fn visible_declaration_by_name<'a>(
    declarations: &'a [Declaration],
    visible_declaration_ids: Option<&FxHashSet<InternedPath>>,
    name: StringId,
) -> Option<&'a Declaration> {
    declarations.iter().rfind(|declaration| {
        declaration.id.name() == Some(name)
            && !declaration.is_unresolved_constant_placeholder()
            && match visible_declaration_ids {
                Some(visible) => visible.contains(&declaration.id),
                None => true,
            }
    })
}

fn builtin_named_type(type_name: StringId, string_table: &StringTable) -> Option<DataType> {
    match string_table.resolve(type_name) {
        "Int" => Some(DataType::Int),
        "Float" => Some(DataType::Float),
        "Bool" => Some(DataType::Bool),
        "String" => Some(DataType::StringSlice),
        "Char" => Some(DataType::Char),
        "ErrorKind" => Some(DataType::BuiltinErrorKind),
        _ => None,
    }
}

#[cfg(test)]
#[path = "tests/type_syntax_tests.rs"]
mod type_syntax_tests;
