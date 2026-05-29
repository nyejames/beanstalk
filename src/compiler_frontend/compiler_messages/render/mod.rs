//! Diagnostic render boundary.
//!
//! WHAT: owns user-visible text generation for typed diagnostics.
//! WHY: frontend stages emit facts; this module is the only normal place where those facts become
//! prose, terminal output, terse records, or dev-server HTML.

pub(crate) mod dev_server;
pub(crate) mod terminal;
pub(crate) mod terse;

mod borrow;
mod context;
mod import_config;
mod payload;
mod syntax;

pub(crate) use borrow::*;
pub(crate) use context::*;
pub(crate) use import_config::*;
pub(crate) use payload::*;
pub(crate) use syntax::*;

use crate::compiler_frontend::basic_utility_functions::normalize_path;
use crate::compiler_frontend::compiler_messages::{
    BorrowAccessKind, CompileTimeEvaluationErrorReason, DeferredFeatureReason, DiagnosticPlace,
    GenericApplicationErrorReason, IncompatibleChoiceComparisonReason,
    InvalidAssignmentTargetReason, InvalidBuiltinCallReason, InvalidChoiceVariantReason,
    InvalidCollectionTypeReason, InvalidCompileTimePathReason, InvalidConfigReason,
    InvalidControlFlowStatementReason, InvalidDeclarationReason, InvalidFieldAccessReason,
    InvalidFunctionSignatureReason, InvalidGenericParameterReason, InvalidImportClauseReason,
    InvalidImportPathReason, InvalidLibraryFolderReason, InvalidMatchPatternReason,
    InvalidMultiBindReason, InvalidMutableAccessReason, InvalidPageMetadataReason,
    InvalidReceiverCallReason, InvalidResultOperandReason, InvalidSignatureMemberReason,
    InvalidTemplateDirectiveReason, InvalidTemplateSlotReason, NameNamespace,
    NamespaceTypeValueMisuseKind, NonExhaustiveMatchReason, PathKind, RangeOperandKind,
    UnsupportedOperatorCategory,
};
use crate::compiler_frontend::datatypes::definitions::TypeDefinition;
use crate::compiler_frontend::datatypes::display::display_type;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::TokenKind;
use std::path::{Path, PathBuf};

pub(crate) fn invalid_signature_member_message(reason: InvalidSignatureMemberReason) -> String {
    match reason {
        InvalidSignatureMemberReason::ChoicePayloadMutable => {
            "Choice payload fields cannot be marked mutable.".to_string()
        }
        InvalidSignatureMemberReason::ChoicePayloadDefaultValue => {
            "Choice payload fields cannot have default values.".to_string()
        }
        InvalidSignatureMemberReason::CompileTimeParameterDeferred => {
            "The '#' binding marker is not supported in parameter or field lists. Compile-time parameters are deferred.".to_string()
        }
        InvalidSignatureMemberReason::ThisNotAllowed => {
            "'this' is reserved for method receiver parameters and is not valid in this context."
                .to_string()
        }
        InvalidSignatureMemberReason::TrailingComma => {
            "Trailing comma is not allowed in this parameter list.".to_string()
        }
    }
}

pub(crate) fn invalid_function_signature_message(
    reason: &InvalidFunctionSignatureReason,
    string_table: &StringTable,
) -> String {
    match reason {
        InvalidFunctionSignatureReason::MissingArrowOrColon { found } => {
            format!(
                "Expected `->` or `:` after function parameters, but found {}.",
                token_kind_name(found, string_table)
            )
        }
        InvalidFunctionSignatureReason::UnexpectedEndAfterParameters => {
            "Function signature ended unexpectedly after parameters.".to_string()
        }
        InvalidFunctionSignatureReason::UnexpectedColonAfterArrow => {
            "Functions without return values must omit the return signature.".to_string()
        }
        InvalidFunctionSignatureReason::TrailingCommaInReturns => {
            "Trailing commas are not allowed in function return declarations.".to_string()
        }
        InvalidFunctionSignatureReason::UnexpectedEndAfterComma => {
            "Function return declarations cannot end after a comma.".to_string()
        }
        InvalidFunctionSignatureReason::UnexpectedEndInReturns => {
            "Unexpected end of function signature while parsing return declarations.".to_string()
        }
        InvalidFunctionSignatureReason::MissingColonAfterReturns => {
            "Function return declarations must end with ':'.".to_string()
        }
        InvalidFunctionSignatureReason::UnexpectedArrowInReturns => {
            "Unexpected '->' inside function return declarations.".to_string()
        }
        InvalidFunctionSignatureReason::MissingCommaOrColon { found } => {
            format!(
                "Expected `,` or `:` after function return declaration, found {}.",
                token_kind_name(found, string_table)
            )
        }
        InvalidFunctionSignatureReason::VoidNotAllowed => {
            "Void is not a valid function return declaration.".to_string()
        }
        InvalidFunctionSignatureReason::UnknownReturnAlias { .. } => {
            "Unknown return alias. Alias returns must name a function parameter.".to_string()
        }
        InvalidFunctionSignatureReason::MissingParameterNameInAlias => {
            "Expected a parameter name in an alias return declaration.".to_string()
        }
        InvalidFunctionSignatureReason::DuplicateParameterInAlias => {
            "Duplicate parameter used in the same alias return declaration.".to_string()
        }
        InvalidFunctionSignatureReason::AliasCannotBeError => {
            "Alias return declarations cannot be marked as an error slot in v1.".to_string()
        }
        InvalidFunctionSignatureReason::MultipleErrorReturnSlots => {
            "Function signatures can only declare one distinguished error return slot.".to_string()
        }
        InvalidFunctionSignatureReason::ErrorSlotNotLast => {
            "The error return slot must be the final return slot in v1.".to_string()
        }
    }
}

pub(crate) fn invalid_generic_application_message(
    reason: GenericApplicationErrorReason,
) -> &'static str {
    match reason {
        GenericApplicationErrorReason::OnNonNamedType => {
            "Generic application can only be used with a named type."
        }
        GenericApplicationErrorReason::EmptyArgumentList => {
            "Generic application requires at least one type argument."
        }
        GenericApplicationErrorReason::MissingArgumentAfterComma => {
            "Generic application is missing a type argument after ','."
        }
        GenericApplicationErrorReason::NestedApplication => {
            "Nested generic type applications are not supported in a single annotation."
        }
    }
}

pub(crate) fn invalid_collection_type_message(reason: InvalidCollectionTypeReason) -> &'static str {
    match reason {
        InvalidCollectionTypeReason::NegativeCapacity => {
            "Collection capacity must be a non-negative integer."
        }
    }
}

pub(crate) fn invalid_generic_parameter_message(
    reason: &InvalidGenericParameterReason,
    string_table: &StringTable,
) -> String {
    match reason {
        InvalidGenericParameterReason::EmptyParameterList
        | InvalidGenericParameterReason::BoundsNotSupported
        | InvalidGenericParameterReason::ListMustStayWithHeader => {
            invalid_generic_parameter_static_message(reason).to_owned()
        }
        InvalidGenericParameterReason::InvalidToken { found } => {
            format!(
                "Invalid generic parameter token {}.",
                token_kind_name(found, string_table)
            )
        }
    }
}

fn invalid_generic_parameter_static_message(
    reason: &InvalidGenericParameterReason,
) -> &'static str {
    match reason {
        InvalidGenericParameterReason::EmptyParameterList => {
            "Expected at least one generic parameter after `type`."
        }
        InvalidGenericParameterReason::BoundsNotSupported => {
            "Generic parameter bounds are not supported yet."
        }
        InvalidGenericParameterReason::ListMustStayWithHeader => {
            "Generic parameter lists must stay with the declaration header."
        }
        InvalidGenericParameterReason::InvalidToken { .. } => "Invalid generic parameter token.",
    }
}

pub(crate) fn invalid_template_directive_message(
    directive_name: Option<StringId>,
    reason: InvalidTemplateDirectiveReason,
    string_table: &StringTable,
) -> String {
    let name = directive_name
        .map(|id| string_table.resolve(id))
        .unwrap_or("unknown");

    match reason {
        InvalidTemplateDirectiveReason::UnknownDirective => {
            format!("Unknown template directive '{name}'.")
        }
        InvalidTemplateDirectiveReason::MissingArgument => {
            format!("Template directive '{name}' is missing a required argument.")
        }
        InvalidTemplateDirectiveReason::InvalidArgument => {
            format!("Invalid argument for template directive '{name}'.")
        }
        InvalidTemplateDirectiveReason::DirectiveNotAllowedHere => {
            format!("Template directive '{name}' is not allowed here.")
        }
    }
}

pub(crate) fn namespace_misuse_message(
    name: StringId,
    expected: NameNamespace,
    found: NameNamespace,
    string_table: &StringTable,
) -> String {
    let name = string_table.resolve(name);

    match (expected, found) {
        (NameNamespace::Type, NameNamespace::Value) => {
            format!("'{name}' is a value and cannot be used as a type.")
        }
        (NameNamespace::Value, NameNamespace::Type) => {
            format!("'{name}' is a type and cannot be used as a value.")
        }
        _ => {
            let expected = namespace_name(expected);
            let found = namespace_name(found);
            format!(
                "'{name}' is in the {found} namespace, but the {expected} namespace was expected."
            )
        }
    }
}

pub(crate) fn unsupported_operator_types_message(
    category: UnsupportedOperatorCategory,
    lhs: TypeId,
    rhs: Option<TypeId>,
    context: DiagnosticRenderContext<'_>,
) -> String {
    let category_name = unsupported_operator_category_name(category);

    if let Some(message) = generic_parameter_operator_message(category_name, lhs, rhs, context) {
        return message;
    }

    if let Some(rhs) = rhs {
        format!(
            "Unsupported operand types for {category_name} operator. Left: {}, Right: {}.",
            diagnostic_type_name(lhs, context),
            diagnostic_type_name(rhs, context)
        )
    } else {
        format!(
            "Unsupported operand type for {category_name} operator. Operand: {}.",
            diagnostic_type_name(lhs, context)
        )
    }
}

fn generic_parameter_operator_message(
    category_name: &str,
    lhs: TypeId,
    rhs: Option<TypeId>,
    context: DiagnosticRenderContext<'_>,
) -> Option<String> {
    let mut parameter_names = Vec::new();

    if let Some(name) = generic_parameter_name(lhs, context) {
        parameter_names.push(name);
    }

    if let Some(rhs) = rhs
        && rhs != lhs
        && let Some(name) = generic_parameter_name(rhs, context)
    {
        parameter_names.push(name);
    }

    if parameter_names.is_empty() {
        return None;
    }

    let subject = if parameter_names.len() == 1 {
        format!("Generic parameter '{}'", parameter_names[0])
    } else {
        format!(
            "Generic parameters {}",
            parameter_names
                .iter()
                .map(|name| format!("'{name}'"))
                .collect::<Vec<_>>()
                .join(" and ")
        )
    };
    let operation_text = if category_name == "this" {
        String::from("this operation")
    } else {
        format!("{category_name} operators")
    };

    Some(format!(
        "{subject} cannot be used with {operation_text} before trait bounds are supported. This operation depends on behavior that is not guaranteed for every generic type. Use a concrete type or wait for trait bounds."
    ))
}

fn generic_parameter_name(type_id: TypeId, context: DiagnosticRenderContext<'_>) -> Option<String> {
    let type_environment = context.type_environment?;
    match type_environment.get(type_id) {
        Some(TypeDefinition::GenericParameter(_)) => Some(diagnostic_type_name(type_id, context)),
        _ => None,
    }
}

pub(crate) fn invalid_result_operand_message(
    reason: InvalidResultOperandReason,
    category: UnsupportedOperatorCategory,
    operand_type: TypeId,
    context: DiagnosticRenderContext<'_>,
) -> String {
    let category_name = unsupported_operator_category_name(category);
    let type_name = diagnostic_type_name(operand_type, context);

    match reason {
        InvalidResultOperandReason::ResultNotUnwrapped => {
            format!(
                "{category_name} operator does not implicitly unwrap Result values (found '{type_name}')."
            )
        }
        InvalidResultOperandReason::OptionNotUnwrapped => {
            format!(
                "{category_name} operator does not implicitly unwrap Option values (found '{type_name}')."
            )
        }
    }
}

fn namespace_name(namespace: NameNamespace) -> &'static str {
    match namespace {
        NameNamespace::Value => "value",
        NameNamespace::Type => "type",
        NameNamespace::Import => "import",
        NameNamespace::Module => "module",
        NameNamespace::Field => "field",
        NameNamespace::Variant => "variant",
        NameNamespace::Function => "function",
        NameNamespace::Method => "method",
        NameNamespace::TemplateSlot => "template slot",
        NameNamespace::ConfigKey => "config key",
    }
}

fn unsupported_operator_category_name(category: UnsupportedOperatorCategory) -> &'static str {
    match category {
        UnsupportedOperatorCategory::Arithmetic => "arithmetic",
        UnsupportedOperatorCategory::Comparison => "comparison",
        UnsupportedOperatorCategory::Range => "range",
        UnsupportedOperatorCategory::Logical => "logical",
        UnsupportedOperatorCategory::Unary => "unary",
        UnsupportedOperatorCategory::Other => "this",
    }
}

pub(crate) fn unsupported_external_function_message(
    function_name: StringId,
    package_path: Option<StringId>,
    backend_name: StringId,
    string_table: &StringTable,
) -> String {
    let function_name = string_table.resolve(function_name);
    let backend_name = string_table.resolve(backend_name);

    if let Some(package_path) = package_path {
        let package_path = string_table.resolve(package_path);
        format!(
            "External function '{function_name}' from package '{package_path}' is not supported by the {backend_name} backend."
        )
    } else {
        format!(
            "External function '{function_name}' is not supported by the {backend_name} backend."
        )
    }
}

pub(crate) fn invalid_choice_variant_message(
    reason: InvalidChoiceVariantReason,
    choice_name: Option<StringId>,
    variant_name: Option<StringId>,
    available_variants: &[StringId],
    string_table: &StringTable,
) -> String {
    let choice_name = choice_name
        .map(|name| string_table.resolve(name).to_owned())
        .unwrap_or_default();
    let variant_name = variant_name
        .map(|name| string_table.resolve(name).to_owned())
        .unwrap_or_default();
    let available_variants = if available_variants.is_empty() {
        String::new()
    } else {
        let names = available_variants
            .iter()
            .map(|name| string_table.resolve(*name).to_owned())
            .collect::<Vec<_>>()
            .join(", ");
        format!(". Available variants: [{names}]")
    };

    match reason {
        InvalidChoiceVariantReason::EmptyRecordBody => {
            "Choice variant record body cannot be empty.".to_owned()
        }
        InvalidChoiceVariantReason::RecursiveDeclaration => {
            "Recursive choice declarations are not supported.".to_owned()
        }
        InvalidChoiceVariantReason::ConstructorStyleNotSupported => {
            "Constructor-style choice declarations are not supported.".to_owned()
        }
        InvalidChoiceVariantReason::PayloadShorthandNotSupported => {
            "Choice payload shorthand is not supported. Use a record body `| field Type, ... |`."
                .to_owned()
        }
        InvalidChoiceVariantReason::UnexpectedSeparator => {
            "Unexpected separator in choice variant declaration.".to_owned()
        }
        InvalidChoiceVariantReason::MissingVariants => {
            "Choice declarations must define at least one variant.".to_owned()
        }
        InvalidChoiceVariantReason::UnknownVariant => {
            format!("Unknown variant '{choice_name}::{variant_name}'{available_variants}.")
        }
        InvalidChoiceVariantReason::UnitVariantWithParentheses => {
            format!(
                "Unit variant '{choice_name}::{variant_name}' cannot be called with empty parentheses."
            )
        }
        InvalidChoiceVariantReason::UnitVariantAsConstructor => {
            format!(
                "Unit variant '{choice_name}::{variant_name}' cannot be called as a constructor."
            )
        }
        InvalidChoiceVariantReason::PayloadVariantMissingArguments => {
            format!(
                "Payload variant '{choice_name}::{variant_name}' requires constructor arguments."
            )
        }
    }
}

pub(crate) fn incompatible_choice_comparison_message(
    reason: &IncompatibleChoiceComparisonReason,
    lhs: TypeId,
    rhs: TypeId,
    context: DiagnosticRenderContext<'_>,
) -> String {
    let lhs_name = diagnostic_type_name(lhs, context);
    let rhs_name = diagnostic_type_name(rhs, context);

    match reason {
        IncompatibleChoiceComparisonReason::DifferentChoiceTypes => {
            format!("Cannot compare choices of different types: '{lhs_name}' and '{rhs_name}'.")
        }
        IncompatibleChoiceComparisonReason::ChoiceWithNonChoice => {
            format!(
                "Cannot compare choice '{lhs_name}' with '{rhs_name}'. Choices can only be compared with values of the same choice type."
            )
        }
        IncompatibleChoiceComparisonReason::PayloadEqualityNotSupported {
            field_name,
            field_type,
        } => {
            let field_name = context.string_table.resolve(*field_name);
            let field_type_name = diagnostic_type_name(*field_type, context);
            format!(
                "Choice payload equality is not supported because field '{field_name}' has type '{field_type_name}', which does not support equality."
            )
        }
    }
}

pub(crate) fn deferred_feature_message(
    reason: &DeferredFeatureReason,
    string_table: &StringTable,
) -> String {
    if let DeferredFeatureReason::NamedFeature { feature } = reason {
        return format!("Deferred feature: {}.", string_table.resolve(*feature));
    }

    deferred_feature_static_message(reason).to_owned()
}

fn deferred_feature_static_message(reason: &DeferredFeatureReason) -> &'static str {
    match reason {
        DeferredFeatureReason::NamedFeature { .. } => "Deferred feature.",
        DeferredFeatureReason::ReservedTraitMustKeyword => {
            "Keyword 'must' is reserved for traits and is deferred for Alpha."
        }
        DeferredFeatureReason::ReservedTraitThisKeyword => {
            "Keyword 'This' is reserved for traits and is deferred for Alpha."
        }
        DeferredFeatureReason::TraitDeclaration => {
            "Trait declarations using 'must' are reserved for traits and are deferred for Alpha."
        }
        DeferredFeatureReason::CaptureTaggedPattern => {
            "Capture/tagged patterns using '|...|' are deferred for Alpha."
        }
        DeferredFeatureReason::NegatedMatchPattern => {
            "Negated match patterns (for example 'not ... =>') are deferred."
        }
        DeferredFeatureReason::NamedPayloadPatternAssignment => {
            "Named payload pattern assignment is deferred. Use positional capture with the field name."
        }
        DeferredFeatureReason::NestedPayloadPattern => {
            "Nested payload patterns are deferred. Use flat capture bindings with declared field names only."
        }
        DeferredFeatureReason::GenericReceiverMethod => {
            "Generic receiver methods are not supported yet. Use a generic free function instead."
        }
        DeferredFeatureReason::PublicOptionTypeSyntax => {
            "Public `Option of T` type syntax is deferred for Alpha. Use the `T?` optional type suffix instead."
        }
        DeferredFeatureReason::PublicResultTypeSyntax => {
            "Public `Result of T, E` type syntax is deferred for Alpha. Use a final `E!` return slot for fallible functions instead."
        }
        DeferredFeatureReason::CheckedBlock => {
            "`checked:` blocks are reserved for future advanced validation, but are not implemented yet."
        }
        DeferredFeatureReason::AsyncBlock => {
            "`async:` blocks are reserved for future async lowering, but are not implemented yet."
        }
    }
}

pub(crate) fn invalid_control_flow_statement_message(
    reason: InvalidControlFlowStatementReason,
) -> String {
    match reason {
        InvalidControlFlowStatementReason::ElseOutsideIfOrMatch => {
            "Unexpected use of 'else' keyword. It can only be used inside an if statement or match statement.".to_string()
        }
        InvalidControlFlowStatementReason::BreakOutsideLoop => {
            "Break statements can only be used inside loops.".to_string()
        }
        InvalidControlFlowStatementReason::ContinueOutsideLoop => {
            "Continue statements can only be used inside loops.".to_string()
        }
        InvalidControlFlowStatementReason::TemplateInsideFunctionBody => {
            "Templates can only be used at the top level, not inside the body of a function.".to_string()
        }
        InvalidControlFlowStatementReason::ReturnOutsideFunction => {
            "Return statements can only be used inside functions.".to_string()
        }
        InvalidControlFlowStatementReason::ReturnBangOutsideErrorFunction => {
            "return! can only be used inside functions that declare an error return slot.".to_string()
        }
        InvalidControlFlowStatementReason::ExpectedColonAfterCondition => {
            "Expected ':' after the condition to open a new scope.".to_string()
        }
        InvalidControlFlowStatementReason::UnexpectedEndOfFileInMatch => {
            "Unexpected end of file in match statement.".to_string()
        }
        InvalidControlFlowStatementReason::CaseRequiredBeforeElse => {
            "Match statements require at least one pattern arm before 'else =>'.".to_string()
        }
        InvalidControlFlowStatementReason::DuplicateElseArm => {
            "Match statement can only have one 'else =>' arm.".to_string()
        }
        InvalidControlFlowStatementReason::ExpectedFatArrow => {
            "Expected '=>' in match arm.".to_string()
        }
        InvalidControlFlowStatementReason::InlineValueIfMultiline => {
            "Inline value-producing 'if' must fit on a single logical line.".to_string()
        }
        InvalidControlFlowStatementReason::InlineValueIfElseThen => {
            "Inline value-producing 'if' cannot use 'else then'.".to_string()
        }
        InvalidControlFlowStatementReason::ValueIfMissingElse => {
            "Value-producing 'if' requires an 'else' branch.".to_string()
        }
        InvalidControlFlowStatementReason::ValueIfBranchFallsThrough => {
            "Every reachable branch of a value-producing 'if' must produce a value or terminate.".to_string()
        }
        InvalidControlFlowStatementReason::ValueIfNoProducingPath => {
            "Value-producing 'if' has no reachable path that produces a value.".to_string()
        }
        InvalidControlFlowStatementReason::ValueBlockOutsideReceiver => {
            "Value-producing blocks are only valid at declaration, assignment, return, multi-bind, catch, or `then` receiving sites.".to_string()
        }
        InvalidControlFlowStatementReason::ValueIfOptionNonePredicate => {
            "Optional `none` checks are statement-only here. Use `if option is |value| ... else ...` for value recovery.".to_string()
        }
        InvalidControlFlowStatementReason::ValueIfOptionLiteralPredicate => {
            "Inline value-producing optional checks must use `|value|`; literal option matching belongs in full `if option is:` matches.".to_string()
        }
    }
}

pub(crate) fn invalid_declaration_message(
    reason: InvalidDeclarationReason,
    name: Option<StringId>,
    string_table: &StringTable,
) -> String {
    let name_text = name
        .map(|name| format!("'{}'", string_table.resolve(name)))
        .unwrap_or_else(|| "This declaration".to_string());

    match reason {
        InvalidDeclarationReason::ReservedBuiltinName => {
            format!("{name_text} is reserved as a builtin language type.")
        }
        InvalidDeclarationReason::ConstantCannotBeMutable => "Constants cannot be mutable.".to_string(),
        InvalidDeclarationReason::ExternalTypeLiteralConstruction => {
            "Cannot construct external type with a struct literal. External types are opaque and can only be obtained from external function calls.".to_string()
        }
        InvalidDeclarationReason::UnusedGenericParameter { parameter_name } => {
            format!(
                "Generic parameter '{}' is declared but never used in the public type shape for '{}'.",
                string_table.resolve(parameter_name),
                name_text.trim_matches('\'')
            )
        }
        InvalidDeclarationReason::RecursiveGenericType => {
            format!(
                "Recursive generic types are not supported yet. Generic type '{}' cannot contain itself.",
                name_text.trim_matches('\'')
            )
        }
        InvalidDeclarationReason::RecursiveRuntimeStruct { cycle } => {
            format!("Recursive runtime struct definitions are not supported in v1. Cycle: {cycle}")
        }
        InvalidDeclarationReason::ExternalTypeAlias { type_name } => {
            let type_name_str = string_table.resolve(type_name);
            format!("Cannot create a type alias for external type '{type_name_str}'. External types are opaque and cannot be aliased.")
        }
        InvalidDeclarationReason::InvalidGenericParameterName { parameter_name } => {
            let parameter_name_str = string_table.resolve(parameter_name);
            format!("Invalid generic parameter name '{parameter_name_str}'. Generic parameter names must be PascalCase or a single uppercase letter.")
        }
        InvalidDeclarationReason::DuplicateGenericParameter { parameter_name } => {
            let parameter_name_str = string_table.resolve(parameter_name);
            format!("Duplicate generic parameter '{parameter_name_str}'. Parameter names must be unique.")
        }
        InvalidDeclarationReason::GenericParameterNameCollision { parameter_name } => {
            let parameter_name_str = string_table.resolve(parameter_name);
            format!("Generic parameter '{parameter_name_str}' collides with an existing visible type name.")
        }
        InvalidDeclarationReason::ReservedGenericParameterName { parameter_name } => {
            let parameter_name_str = string_table.resolve(parameter_name);
            format!("Generic parameter '{parameter_name_str}' collides with a builtin type name.")
        }
    }
}

pub(crate) fn invalid_generic_instantiation_message(
    type_name: Option<StringId>,
    reason: &crate::compiler_frontend::compiler_messages::InvalidGenericInstantiationReason,
    string_table: &StringTable,
) -> String {
    use crate::compiler_frontend::compiler_messages::InvalidGenericInstantiationReason;

    let type_name_str = type_name
        .map(|n| format!("'{}'", string_table.resolve(n)))
        .unwrap_or_else(|| "This type".to_string());

    match reason {
        InvalidGenericInstantiationReason::WrongArgumentCount { expected, found } => {
            format!(
                "Generic type {type_name_str} expects {expected} type argument{}, but {found} were provided.",
                if *expected == 1 { "" } else { "s" }
            )
        }
        InvalidGenericInstantiationReason::TypeDoesNotAcceptArguments => {
            format!("Type {type_name_str} does not accept generic arguments.")
        }
        InvalidGenericInstantiationReason::MissingTypeArguments => {
            format!("Generic type {type_name_str} requires type arguments.")
        }
        InvalidGenericInstantiationReason::CannotInferArguments { missing_parameters } => {
            let missing = missing_parameters
                .iter()
                .map(|p| string_table.resolve(*p))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Cannot infer type argument(s) for generic type {type_name_str}: {missing}. Provide an explicit type annotation or constructor arguments with concrete types."
            )
        }
        InvalidGenericInstantiationReason::CannotInferFunctionArguments { missing_parameters } => {
            let missing = missing_parameters
                .iter()
                .map(|p| string_table.resolve(*p))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Cannot infer type argument(s) for generic function {type_name_str}: {missing}. Add an immediate type annotation or pass arguments that fix the type."
            )
        }
        InvalidGenericInstantiationReason::ConflictingFunctionArgument { parameter_name } => {
            let parameter = string_table.resolve(*parameter_name);
            format!(
                "Generic function {type_name_str} infers conflicting concrete types for type parameter '{parameter}'."
            )
        }
        InvalidGenericInstantiationReason::RecursiveFunctionInstantiation => {
            format!(
                "Generic function {type_name_str} recursively instantiates itself, which is deferred for Alpha."
            )
        }
        InvalidGenericInstantiationReason::GenericFunctionValueDeferred => {
            format!(
                "Generic function {type_name_str} must be called; generic function values are deferred for Alpha."
            )
        }
    }
}

pub(crate) fn invalid_range_operand_message(
    operand: RangeOperandKind,
    found_type: TypeId,
    context: DiagnosticRenderContext<'_>,
) -> String {
    let operand_name = match operand {
        RangeOperandKind::Start => "start",
        RangeOperandKind::End => "end",
        RangeOperandKind::Step => "step",
    };
    let type_name = diagnostic_type_name(found_type, context);

    format!("Range {operand_name} must be numeric (Int or Float). Found '{type_name}'")
}

pub(crate) fn unsupported_builder_package_message(
    package_path: StringId,
    string_table: &StringTable,
) -> String {
    let package_path_str = string_table.resolve(package_path);
    format!(
        "Core package '{package_path_str}' is not supported by this builder. \
         Use a builder that exposes this core package or remove the import."
    )
}

pub(crate) fn invalid_page_metadata_message(
    key: StringId,
    reason: InvalidPageMetadataReason,
    string_table: &StringTable,
) -> String {
    let key_str = string_table.resolve(key);
    match reason {
        InvalidPageMetadataReason::NotAString => {
            format!("Reserved HTML page metadata constant '{key_str}' must fold to a string.")
        }
        InvalidPageMetadataReason::DuplicateDeclaration => {
            format!(
                "Reserved HTML page metadata constant '{key_str}' is declared more than once for this entry page."
            )
        }
    }
}

pub(crate) fn invalid_expression_message() -> String {
    "Invalid expression: no valid operands found during evaluation.".to_string()
}

/// Determine which special file name is referenced by an import path.
///
/// WHAT: inspects path components to find `#mod`, `#page`, or `#config` references.
/// WHY: the direct-special-file diagnostic covers all special files, and renderers
/// should name the specific file when possible.
pub(crate) fn special_file_name_from_path(
    path: &InternedPath,
    string_table: &StringTable,
) -> &'static str {
    for component in path.as_components() {
        let segment = string_table.resolve(*component);
        if segment == "#mod" || segment == "#mod.bst" {
            return "#mod.bst";
        }
        if segment == "#page" || segment == "#page.bst" {
            return "#page.bst";
        }
        if segment == "#config" || segment == "#config.bst" {
            return "#config.bst";
        }
    }
    "special file"
}

pub(crate) fn invalid_receiver_declaration_message(
    reason: crate::compiler_frontend::compiler_messages::InvalidReceiverDeclarationReason,
    string_table: &StringTable,
) -> String {
    use crate::compiler_frontend::compiler_messages::InvalidReceiverDeclarationReason;

    match reason {
        InvalidReceiverDeclarationReason::UnknownStructTarget => {
            "Receiver method targets an unknown struct.".to_string()
        }
        InvalidReceiverDeclarationReason::WrongSourceFile => {
            "Receiver method must be declared in the same file as the struct definition."
                .to_string()
        }
        InvalidReceiverDeclarationReason::FieldNameConflict => {
            "Struct declares both a field and a method with the same name.".to_string()
        }
        InvalidReceiverDeclarationReason::DuplicateMethod => {
            "Duplicate receiver method for this receiver and name.".to_string()
        }
        InvalidReceiverDeclarationReason::ImportedReceiverTypeNotVisible => {
            "Imported receiver method is visible without its receiver type. Import the receiver type from the same surface or import the whole namespace.".to_string()
        }
        InvalidReceiverDeclarationReason::ImportedMethodCollision => {
            "Imported receiver methods collide for the same receiver type and method name."
                .to_string()
        }
        InvalidReceiverDeclarationReason::GenericReceiverType {
            function_name,
            type_name,
        } => {
            format!(
                "Function '{}' uses generic receiver type '{}'. Receiver methods on generic types are not supported yet.",
                string_table.resolve(function_name),
                string_table.resolve(type_name)
            )
        }
        InvalidReceiverDeclarationReason::UnsupportedType {
            function_name,
            type_name,
        } => {
            format!(
                "Function '{}' uses unsupported receiver type '{}'. Receiver methods must target a user-defined struct or built-in scalar type.",
                string_table.resolve(function_name),
                string_table.resolve(type_name)
            )
        }
    }
}

pub(crate) fn invalid_assignment_target_message(
    reason: InvalidAssignmentTargetReason,
    target_name: Option<StringId>,
    target_type: Option<TypeId>,
    context: DiagnosticRenderContext<'_>,
) -> String {
    let string_table = context.string_table;
    let target_text = named_value_or_default(target_name, string_table, "this target");
    let target_type_text = target_type.map(|type_id| diagnostic_type_name(type_id, context));

    match reason {
        InvalidAssignmentTargetReason::NotMutablePlace => match target_type_text {
            Some(target_type_text) => {
                format!(
                    "Field assignment requires a mutable place receiver. '{target_type_text}' is a temporary expression, not a mutable place."
                )
            }
            None => {
                "Field assignment requires a mutable place receiver. Writing through temporaries or other rvalues is not allowed.".to_string()
            }
        },
        InvalidAssignmentTargetReason::ImmutableVariable => {
            format!("Cannot mutate immutable variable {target_text}. Use '~' to declare a mutable variable.")
        }
        InvalidAssignmentTargetReason::UnavailableInCatchRecovery => {
            format!(
                "Assignment target {target_text} is unavailable inside catch recovery for the same assignment."
            )
        }
        InvalidAssignmentTargetReason::CollectionIndexedWriteRemoved => {
            "Indexed assignment through collection `get(...)` has been removed. Use `~items.set(index, value)!` or handle `~items.set(index, value) catch:` instead.".to_string()
        }
        InvalidAssignmentTargetReason::ExpectedAssignmentOperator => {
            format!("Expected assignment operator after variable {target_text}.")
        }
    }
}

pub(crate) fn invalid_multi_bind_message(
    reason: InvalidMultiBindReason,
    target_name: Option<StringId>,
    string_table: &StringTable,
) -> String {
    let target_text = named_value_or_default(target_name, string_table, "this target");

    match reason {
        InvalidMultiBindReason::ThisTargetReserved => {
            "'this' is reserved for method receiver parameters and cannot be used in multi-bind declarations.".to_string()
        }
        InvalidMultiBindReason::ExpectedTargetName => {
            "Malformed multi-bind target list. Expected a symbol target name.".to_string()
        }
        InvalidMultiBindReason::MissingTargetAfterComma => {
            "Malformed multi-bind target list near ','.".to_string()
        }
        InvalidMultiBindReason::MissingAssignmentOperator => {
            "Multi-bind target list is missing a shared '=' assignment operator.".to_string()
        }
        InvalidMultiBindReason::InvalidTokenAfterTarget => {
            "Invalid token after multi-bind target.".to_string()
        }
        InvalidMultiBindReason::MissingRightHandExpression => {
            "Multi-bind statement is missing a right-hand expression after '='.".to_string()
        }
        InvalidMultiBindReason::MultipleRightHandExpressions => {
            "Multi-bind statements accept exactly one right-hand expression.".to_string()
        }
        InvalidMultiBindReason::MutableTargetNeedsExplicitType => {
            format!("Mutable multi-bind target {target_text} requires an explicit type annotation.")
        }
        InvalidMultiBindReason::DuplicateTarget => {
            format!("Duplicate multi-bind target {target_text} in the same target list.")
        }
        InvalidMultiBindReason::UnsupportedRhs => {
            "Multi-bind is only supported for explicit multi-value surfaces. For now, that means multi-return function calls.".to_string()
        }
        InvalidMultiBindReason::ExistingTargetMutableMarker => {
            format!("Existing multi-bind target {target_text} cannot use a mutable marker.")
        }
        InvalidMultiBindReason::ExistingTargetImmutable => {
            format!("Existing multi-bind target {target_text} is immutable and cannot be reassigned.")
        }
        InvalidMultiBindReason::ArityMismatch { expected, found } => {
            format!("Multi-bind arity mismatch: {expected} target(s) but {found} value slot(s). Target count must match returned slot count exactly.")
        }
        InvalidMultiBindReason::RhsNotMultiValue => {
            "Multi-bind right-hand expression must evaluate to a multi-value return pack.".to_string()
        }
    }
}

pub(crate) fn invalid_builtin_call_message(
    reason: InvalidBuiltinCallReason,
    builtin_name: Option<StringId>,
    string_table: &StringTable,
) -> String {
    let builtin_text = named_value_or_default(builtin_name, string_table, "This builtin");

    match reason {
        InvalidBuiltinCallReason::MissingParentheses => {
            format!("{builtin_text} method call is missing '(' before the argument list.")
        }
        InvalidBuiltinCallReason::TakesNoArguments => {
            format!("{builtin_text} method takes no arguments.")
        }
        InvalidBuiltinCallReason::NamedArgumentsNotSupported => {
            "Named arguments are not supported for builtin member calls.".to_string()
        }
        InvalidBuiltinCallReason::MustHandleFallibleResult => {
            format!(
                "Calls to {builtin_text} must be explicitly handled with postfix `!` or `catch`."
            )
        }
        InvalidBuiltinCallReason::DoesNotAcceptMutableAccess => {
            format!("{builtin_text} does not accept explicit mutable access marker '~'.")
        }
        InvalidBuiltinCallReason::CastMissingParentheses => {
            format!("{builtin_text} cast requires parentheses and exactly one argument.")
        }
        InvalidBuiltinCallReason::CastMissingArgument => {
            format!("{builtin_text} cast requires exactly one argument.")
        }
        InvalidBuiltinCallReason::CastTooManyArguments => {
            format!("{builtin_text} cast takes exactly one argument.")
        }
        InvalidBuiltinCallReason::CastMissingClosingParenthesis => {
            format!("Expected ')' after {builtin_text} cast argument.")
        }
        InvalidBuiltinCallReason::MissingArgument => {
            format!("{builtin_text} requires at least one argument.")
        }
        InvalidBuiltinCallReason::TooManyArguments => {
            format!("{builtin_text} takes too many arguments.")
        }
        InvalidBuiltinCallReason::RuntimeMessageExpressionDeferred => {
            "Assertion messages must be string literals; runtime message expressions are deferred."
                .to_string()
        }
        InvalidBuiltinCallReason::ExpressionPositionNotAllowed => {
            format!("{builtin_text} is a statement and cannot be used in expression position.")
        }
    }
}

pub(crate) fn invalid_receiver_call_message(
    reason: InvalidReceiverCallReason,
    receiver_type: Option<StringId>,
    method_name: Option<StringId>,
    string_table: &StringTable,
) -> String {
    let receiver_text = named_value_or_default(receiver_type, string_table, "this receiver");
    let method_text = named_value_or_default(method_name, string_table, "this method");

    match reason {
        InvalidReceiverCallReason::CalledAsFreeFunction => {
            format!("{method_text} is a receiver method for {receiver_text} and cannot be called as a free function.")
        }
        InvalidReceiverCallReason::MustUseParentheses => {
            format!("{method_text} is a receiver method and must be called with parentheses.")
        }
        InvalidReceiverCallReason::ConstStructNoRuntimeCalls => {
            format!(
                "Const struct records are data-only and do not support runtime method calls like {method_text}."
            )
        }
        InvalidReceiverCallReason::MutablePlaceRequired => {
            format!("Mutable receiver method {method_text} requires a mutable place receiver.")
        }
        InvalidReceiverCallReason::MutableCollectionRequired => {
            format!(
                "Collection mutating method {method_text} requires a mutable collection receiver."
            )
        }
        InvalidReceiverCallReason::MissingMutableAccessMarker => {
            format!(
                "{method_text} expects mutable access at the receiver call site. Call this with '~{receiver_text}'."
            )
        }
        InvalidReceiverCallReason::UnneededMutableAccessMarker => {
            format!("{method_text} does not accept explicit mutable access marker '~'.")
        }
        InvalidReceiverCallReason::MutableMarkerOnNonReceiverCall => {
            "Mutable receiver marker '~' is only valid for receiver calls like '~value.method(...)' or '~values.push(...)'."
                .to_string()
        }
    }
}

pub(crate) fn invalid_copy_target_message(
    reason: crate::compiler_frontend::compiler_messages::InvalidCopyTargetReason,
) -> String {
    match reason {
        crate::compiler_frontend::compiler_messages::InvalidCopyTargetReason::FunctionValue => {
            "The 'copy' keyword only accepts places, not function values or calls.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidCopyTargetReason::NonPlace => {
            "The 'copy' keyword only accepts a place expression.".to_string()
        }
    }
}

pub(crate) fn invalid_field_access_message(
    reason: InvalidFieldAccessReason,
    field_name: Option<StringId>,
    receiver_type: Option<TypeId>,
    context: DiagnosticRenderContext<'_>,
) -> String {
    let string_table = context.string_table;
    let field_text = named_value_or_default(field_name, string_table, "this field");
    let receiver_text = receiver_type.map(|type_id| diagnostic_type_name(type_id, context));

    match reason {
        InvalidFieldAccessReason::ExpectedNameAfterDot => {
            format!("Expected property or method name after '.', found {field_text}.")
        }
        InvalidFieldAccessReason::FieldNotMethod => {
            format!(
                "{field_text} is a field, not a receiver method. Dot-call syntax is reserved for declared receiver methods."
            )
        }
        InvalidFieldAccessReason::ChoicePayloadMutation => {
            "Choice payload fields are immutable. Mutation is not supported.".to_string()
        }
        InvalidFieldAccessReason::ChoicePayloadDeferred => {
            "Choice payload field access is deferred. Use pattern matching to extract payload fields."
                .to_string()
        }
        InvalidFieldAccessReason::UnknownExternalMember => match receiver_text {
            Some(receiver_text) => {
                format!("Property or method {field_text} not found for external type '{receiver_text}'.")
            }
            None => format!("Property or method {field_text} not found for external type."),
        },
        InvalidFieldAccessReason::UnknownMember => match receiver_text {
            Some(receiver_text) => {
                format!("Property or method {field_text} not found for '{receiver_text}'.")
            }
            None => format!("Property or method {field_text} not found."),
        },
    }
}

pub(crate) fn invalid_match_pattern_message(
    reason: InvalidMatchPatternReason,
    variant_name: Option<StringId>,
    string_table: &StringTable,
) -> String {
    let variant_text = named_value_or_default(variant_name, string_table, "this variant");

    match reason {
        InvalidMatchPatternReason::WildcardNotSupported => {
            "Wildcard pattern '_' is not supported in Beanstalk. Use 'else =>' for a catch-all arm.".to_string()
        }
        InvalidMatchPatternReason::AsNotValid => {
            "`as` is not valid in match patterns. It is only supported in choice payload captures.".to_string()
        }
        InvalidMatchPatternReason::NegativeLiteralNotNumeric => {
            "Negative literal patterns must be numeric literals, for example '-1' or '-3.2'.".to_string()
        }
        InvalidMatchPatternReason::LiteralTypeUnsupported => {
            "Literal match patterns currently support only literal int, float, bool, char, and string values.".to_string()
        }
        InvalidMatchPatternReason::ScrutineeTypeUnsupportedForRelational => {
            "Relational match patterns are only supported for ordered scalar types: Int, Float, Char, and String.".to_string()
        }
        InvalidMatchPatternReason::UnitVariantHasPayload => {
            format!("Unit variant {variant_text} cannot have payload captures. Use '<variant> =>' without parentheses.")
        }
        InvalidMatchPatternReason::PayloadVariantNeedsBindings => {
            format!("Payload variant {variant_text} requires capture bindings. Expected '{variant_text}(...) =>'.")
        }
        InvalidMatchPatternReason::CaptureBindingMustBeFieldName => {
            "Capture binding must be a field name.".to_string()
        }
        InvalidMatchPatternReason::ExpectedLocalBindingAfterAs => {
            "Expected local binding name after `as` in choice payload pattern.".to_string()
        }
        InvalidMatchPatternReason::AliasMustBeLocalBinding => {
            "Choice payload alias must be a local binding name.".to_string()
        }
        InvalidMatchPatternReason::DuplicateCaptureBinding => {
            format!("Duplicate capture binding in pattern for variant {variant_text}.")
        }
        InvalidMatchPatternReason::TooManyCaptureBindings => {
            format!("Too many capture bindings for variant {variant_text}.")
        }
        InvalidMatchPatternReason::CaptureBindingNameMismatch => {
            format!("Capture binding does not match payload field name in variant {variant_text}.")
        }
        InvalidMatchPatternReason::TooFewCaptureBindings => {
            format!("Too few capture bindings for variant {variant_text}.")
        }
        InvalidMatchPatternReason::QualifierDoesNotMatchScrutinee => {
            "Match arm qualifier does not match the scrutinee choice.".to_string()
        }
        InvalidMatchPatternReason::ExpectedVariantNameAfterQualifier => {
            "Expected a variant name after '::' in this match pattern.".to_string()
        }
        InvalidMatchPatternReason::MustUseVariantNamesNotLiterals => {
            "Choice match arms must use variant names, not raw literals.".to_string()
        }
        InvalidMatchPatternReason::MustStartWithVariantName => {
            "Choice match arms must start with a declared variant name.".to_string()
        }
        InvalidMatchPatternReason::UnknownVariant => format!("Unknown variant {variant_text}."),
        InvalidMatchPatternReason::CaptureBindingShadowsVariable => {
            "Capture binding shadows an existing variable. Beanstalk does not allow shadowing.".to_string()
        }
        InvalidMatchPatternReason::NonePatternRequiresOptionalScrutinee => {
            "`none =>` is only valid when matching an optional value.".to_string()
        }
        InvalidMatchPatternReason::OptionValuePatternRequiresEquality => {
            "Option value patterns require the option's inner type to support equality.".to_string()
        }
        InvalidMatchPatternReason::BareCaptureOnOptionalScrutinee => {
            "A bare capture name is not allowed on an optional scrutinee. Use `|name|` to capture the present value.".to_string()
        }
        InvalidMatchPatternReason::OptionPresentCaptureOnNonOptional => {
            "`|name|` capture is only valid when matching an optional value.".to_string()
        }
        InvalidMatchPatternReason::EmptyOptionPresentCapture => {
            "Option present capture cannot be empty. Use `|name|` to capture the present value.".to_string()
        }
        InvalidMatchPatternReason::OptionPresentCaptureTypeAnnotation => {
            "Type annotations are not allowed inside `|...|`.".to_string()
        }
        InvalidMatchPatternReason::MissingClosingPipe => {
            "Expected `|` to close the option present capture.".to_string()
        }
        InvalidMatchPatternReason::ExpectedBindingInOptionPresentCapture => {
            "Expected a binding name inside `|...|`.".to_string()
        }
    }
}

pub(crate) fn non_exhaustive_match_message(
    reason: NonExhaustiveMatchReason,
    missing_variants: &[StringId],
    string_table: &StringTable,
) -> String {
    match reason {
        NonExhaustiveMatchReason::MissingElseArm => {
            "Choice matches with guarded arms must include an explicit 'else =>' arm.".to_string()
        }
        NonExhaustiveMatchReason::MissingVariants => {
            let variants = missing_variants
                .iter()
                .map(|variant| string_table.resolve(*variant).to_string())
                .collect::<Vec<_>>()
                .join(", ");

            format!("Non-exhaustive choice match. Missing variants: [{variants}].")
        }
        NonExhaustiveMatchReason::GuardedArmsRequireElse => {
            "Choice matches with guarded arms must include an explicit 'else =>' arm in Alpha."
                .to_string()
        }
        NonExhaustiveMatchReason::MissingOptionPatterns => {
            "Non-exhaustive option match. Add `none =>` or `|name| =>` to cover all cases."
                .to_string()
        }
    }
}

pub(crate) fn invalid_template_slot_message(
    reason: InvalidTemplateSlotReason,
    slot_name: Option<StringId>,
    string_table: &StringTable,
) -> String {
    let slot_text = named_value_or_default(slot_name, string_table, "this slot");

    match reason {
        InvalidTemplateSlotReason::InsertOutsideParentSlot => {
            "$insert(...) can only be used while filling an immediate parent template that defines matching $slot targets.".to_string()
        }
        InvalidTemplateSlotReason::ExtraLooseContentWithoutDefaultSlot => {
            "This template defines positional $slot(n) targets but no default $slot. There is more loose content than positional slots available.".to_string()
        }
        InvalidTemplateSlotReason::LooseContentWithoutDefaultSlot => {
            "This template defines named $slot(...) targets without a default $slot. Loose content is not allowed here; use $insert(\"name\").".to_string()
        }
        InvalidTemplateSlotReason::InsertCannotTargetDefaultSlot => {
            "$insert cannot target the default slot because the parent template does not define $slot.".to_string()
        }
        InvalidTemplateSlotReason::InsertTargetsUnknownNamedSlot => {
            format!("$insert({slot_text}) targets a named slot that does not exist on the immediate parent template.")
        }
        InvalidTemplateSlotReason::InsertTargetsUnknownPositionalSlot => {
            "$insert targets a positional slot that does not exist on the immediate parent template.".to_string()
        }
        InvalidTemplateSlotReason::MultipleDefaultSlots => {
            "Templates can only define one default $slot.".to_string()
        }
        InvalidTemplateSlotReason::SlotDefinitionOutsideTemplateBody => {
            "$slot markers are only valid as direct nested templates inside template bodies.".to_string()
        }
    }
}

pub(crate) fn compile_time_evaluation_error_message(
    reason: CompileTimeEvaluationErrorReason,
    operation: Option<StringId>,
    string_table: &StringTable,
) -> String {
    let operation_text = named_value_or_default(operation, string_table, "this expression");

    match reason {
        CompileTimeEvaluationErrorReason::IntegerOverflow => {
            format!("Compile-time integer overflow while evaluating {operation_text}.")
        }
        CompileTimeEvaluationErrorReason::FloatOverflow => {
            format!(
                "Compile-time float overflow or non-finite result while evaluating {operation_text}."
            )
        }
        CompileTimeEvaluationErrorReason::DivideByZero => "Cannot divide by zero.".to_string(),
        CompileTimeEvaluationErrorReason::InvalidOperatorForType => {
            format!("Cannot perform operation {operation_text} on this type.")
        }
        CompileTimeEvaluationErrorReason::IntegerDivisionOnlyIntInt => {
            "Integer division operator '//' only supports Int and Int operands.".to_string()
        }
        CompileTimeEvaluationErrorReason::InvalidNumericCast => {
            let detail = operation
                .map(|operation| string_table.resolve(operation))
                .unwrap_or("the cast input is invalid");
            format!("Cannot evaluate this numeric cast at compile time: {detail}.")
        }
        CompileTimeEvaluationErrorReason::ConstantSelfReference => {
            format!("Constant {operation_text} cannot reference itself in its initializer.")
        }
        CompileTimeEvaluationErrorReason::ConstantNotVisible => {
            format!("Constant {operation_text} is not visible in this file.")
        }
        CompileTimeEvaluationErrorReason::NonConstantReferenceInConstant => {
            format!(
                "Constants can only reference other constants. {operation_text} resolves to a non-constant value."
            )
        }
        CompileTimeEvaluationErrorReason::SameFileForwardConstantReference => {
            format!(
                "Constant initializer references same-file constant {operation_text} before it is declared."
            )
        }
        CompileTimeEvaluationErrorReason::ConstantInitializerNotFoldable => {
            format!("Constant {operation_text} is not compile-time resolvable.")
        }
        CompileTimeEvaluationErrorReason::ExternalNonScalarConstantInConstantContext => {
            format!(
                "External constant {operation_text} is not a scalar value and cannot be used in a constant context."
            )
        }
        CompileTimeEvaluationErrorReason::ExternalFunctionCallInConstantContext => {
            format!(
                "Constants cannot call external functions. {operation_text} is a runtime external call."
            )
        }
        CompileTimeEvaluationErrorReason::NonCompileTimeFieldInConstantContext => {
            format!(
                "Const coercion requires compile-time field values. {operation_text} is not compile-time constant."
            )
        }
        CompileTimeEvaluationErrorReason::NoneLiteralRequiresOptionalTypeContext => {
            "The 'none' literal requires an explicit optional type context.".to_string()
        }
        CompileTimeEvaluationErrorReason::ExternalTypeConstructionNotSupported => {
            format!(
                "Cannot construct external type {operation_text} with a struct literal. External types are opaque and can only be obtained from external function calls."
            )
        }
        CompileTimeEvaluationErrorReason::StructFieldDefaultNotFoldable => {
            format!("Struct field default value {operation_text} is not compile-time resolvable.")
        }
    }
}

pub(crate) fn compile_time_evaluation_error_suggestion(
    reason: CompileTimeEvaluationErrorReason,
) -> &'static str {
    match reason {
        CompileTimeEvaluationErrorReason::IntegerOverflow
        | CompileTimeEvaluationErrorReason::FloatOverflow => {
            "Use smaller values or avoid compile-time evaluation of large expressions"
        }
        CompileTimeEvaluationErrorReason::DivideByZero => {
            "Avoid division by zero in compile-time expressions"
        }
        CompileTimeEvaluationErrorReason::InvalidOperatorForType => {
            "Use an operator that is valid for the operand types"
        }
        CompileTimeEvaluationErrorReason::IntegerDivisionOnlyIntInt => {
            "Use '//' only with two Int operands"
        }
        CompileTimeEvaluationErrorReason::InvalidNumericCast => {
            "Use a numeric value that can be represented by the target type"
        }
        CompileTimeEvaluationErrorReason::ConstantSelfReference => {
            "A constant cannot depend on itself. Use a different value or compute it differently."
        }
        CompileTimeEvaluationErrorReason::ConstantNotVisible => {
            "Import the compile-time constant before using it in this constant initializer."
        }
        CompileTimeEvaluationErrorReason::NonConstantReferenceInConstant => {
            "Only reference constants in constant declarations and const templates."
        }
        CompileTimeEvaluationErrorReason::SameFileForwardConstantReference => {
            "Move the referenced constant above this declaration, or import it from another file."
        }
        CompileTimeEvaluationErrorReason::ConstantInitializerNotFoldable => {
            "Constants may only contain compile-time values and constant references."
        }
        CompileTimeEvaluationErrorReason::ExternalNonScalarConstantInConstantContext => {
            "Only scalar external constants (Int, Float, Bool) are supported in constant declarations and const templates"
        }
        CompileTimeEvaluationErrorReason::ExternalFunctionCallInConstantContext => {
            "Use only compile-time constant values inside constants and const templates"
        }
        CompileTimeEvaluationErrorReason::NonCompileTimeFieldInConstantContext => {
            "Use only compile-time values when constructing records or choices for top-level '#' constants"
        }
        CompileTimeEvaluationErrorReason::NoneLiteralRequiresOptionalTypeContext => {
            "Add an explicit optional type annotation (e.g., 'value Option<Type> = none')"
        }
        CompileTimeEvaluationErrorReason::ExternalTypeConstructionNotSupported => {
            "Use an external function that returns this type instead"
        }
        CompileTimeEvaluationErrorReason::StructFieldDefaultNotFoldable => {
            "Struct field defaults may only contain compile-time values and constant references."
        }
    }
}

pub(crate) fn invalid_template_structure_message(
    reason: crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason,
) -> String {
    match reason {
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MissingClosingBracket => {
            "Template is missing a closing bracket.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::SlotInHead => {
            "Slot insertions cannot appear in template heads.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MissingHandlerBody => {
            "Template handler is missing a body.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::InvalidChildDirective => {
            "Invalid child directive in template.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::NestedTemplateNotAllowed => {
            "Nested templates are not allowed here.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::HelperInConstTemplate => {
            "Top-level const templates cannot evaluate to '$insert(...)' helpers.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::NonFoldableConstTemplate => {
            "Top-level const templates must be fully foldable at compile time.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::NonFoldableDocComment => {
            "'$doc' comments can only contain compile-time values.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::ResultInTemplateHead => {
            "Template head expressions do not implicitly unwrap Result values.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::UnsupportedTypeInTemplateHead { .. } => {
            "Template head expressions only accept final scalar or textual values.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::RuntimeTemplateInConst => {
            "Const templates can only capture compile-time templates.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::RuntimeValueInConstTemplateHead => {
            "Const templates can only capture compile-time values in the template head.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::EmptyPathInTemplateHead => {
            "Path token in template head cannot be empty.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::PathAliasInTemplateHead => {
            "Path aliases are only valid in import clauses.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::IncompatibleHeadItem => {
            "This template head item is incompatible with other meaningful items in this template head.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::HelperOutsideWrapperSlot => {
            "Template helper reached AST finalization outside immediate wrapper-slot composition.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::RuntimeControlFlowUnresolvedSlot => {
            "Runtime template control-flow bodies cannot leave unresolved `$slot` placeholders.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::RuntimeControlFlowUnresolvedInsert => {
            "Runtime template control-flow bodies cannot leave unresolved `$insert(...)` helpers.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MissingCommaBeforeControlFlowSuffix => {
            "Template control-flow suffixes must be separated from earlier head items with a comma.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::ControlFlowSuffixNotFinal => {
            "Template control-flow suffixes must be the final item in the template head.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MissingTemplateIfCondition => {
            "Template `if` suffix is missing a condition.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MissingTemplateLoopHeader => {
            "Template `loop` suffix is missing a range or collection header.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::ElseInTemplateHead => {
            "`else` is only valid as a standalone template body sentinel `[else]` inside a template `if`.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::OrphanTemplateElse => {
            "Template `[else]` is only valid inside a template `if` body.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::OrphanTemplateElseIf => {
            "Template `[else if ...]` is only valid inside a template `if` body before the final `[else]`.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::OrphanTemplateBreak => {
            "Template `[break]` is only valid inside a template `loop` body.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::OrphanTemplateContinue => {
            "Template `[continue]` is only valid inside a template `loop` body.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::DuplicateTemplateElse => {
            "Template `if` bodies can only contain one direct `[else]` sentinel.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateElseIfAfterElse => {
            "Template `[else if ...]` must appear before the final `[else]` branch.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MalformedTemplateElse => {
            "Template `else` must use the exact standalone form `[else]`.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MalformedTemplateElseIf => {
            "Template `else if` must use the standalone form `[else if condition]` without a body colon.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MalformedTemplateBreak => {
            "Template loop control must use the exact standalone form `[break]`.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MalformedTemplateContinue => {
            "Template loop control must use the exact standalone form `[continue]`.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::MissingTemplateElseIfCondition => {
            "Template `[else if ...]` is missing a condition.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::InlineTemplateElse => {
            "Template `[else]` must be standalone, with no meaningful same-line body text beside it.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::InlineTemplateElseIf => {
            "Template `[else if ...]` must be standalone, with no meaningful same-line body text beside it.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::InlineTemplateBreak => {
            "Template `[break]` must be standalone, with no meaningful same-line body text beside it.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::InlineTemplateContinue => {
            "Template `[continue]` must be standalone, with no meaningful same-line body text beside it.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateElseInLiteralBody => {
            "Template `[else]` cannot split a template body whose directive treats bracketed content as literal text.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateElseIfInLiteralBody => {
            "Template `[else if ...]` cannot split a template body whose directive treats bracketed content as literal text.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateLoopControlInLiteralBody => {
            "Template `[break]` and `[continue]` cannot control a template body whose directive treats bracketed content as literal text.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateElseInLoopBody => {
            "Template `[else]` cannot appear directly inside a template `loop` body.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateElseIfInLoopBody => {
            "Template `[else if ...]` cannot appear directly inside a template `loop` body.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::UnexpectedTokenAfterControlFlowSuffix => {
            "Unexpected token after template control-flow suffix.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateMatchStyleControlFlowRemoved => {
            "Match-style template control flow (`if value is:`) is not supported. Use a boolean condition or option-present capture in template `if` suffixes.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateIfConditionNotConst => {
            "Template `if` condition in a const-required template must fold to a Bool at compile time.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateIfBranchNotConst => {
            "Both branches of a const-required template `if` must be fully foldable, even when one branch is inactive.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateOptionCaptureConstDeferred => {
            "Option-present template `if` folding in const-required templates is deferred because the current const value model cannot decide option presence here.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateLoopRangeBoundsNotConst => {
            "Template range loop bounds in a const-required template must fold to numeric values at compile time.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateLoopSourceNotConst => {
            "Template collection loop source in a const-required template must fold to a collection at compile time.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateLoopConditionNotConst => {
            "Template conditional loop condition in a const-required template must fold to a Bool at compile time.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateConditionalLoopConstTrue => {
            "Const-required template conditional loops with a true condition are rejected because they may not terminate.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateLoopBodyNotConst => {
            "Template loop body in a const-required template must be fully foldable for every iteration.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidTemplateStructureReason::TemplateConstLoopExpansionLimitExceeded { limit } => {
            format!(
                "Const template loop expansion is limited to {} iterations.",
                limit
            )
        }
    }
}

pub(crate) fn invalid_call_shape_message(
    reason: crate::compiler_frontend::compiler_messages::InvalidCallShapeReason,
) -> String {
    match reason {
        crate::compiler_frontend::compiler_messages::InvalidCallShapeReason::MissingArgument { .. } => {
            "Missing argument for a parameter in this call.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidCallShapeReason::ExtraPositionalArgument { expected_count } => {
            format!("This call provides more than {expected_count} positional argument(s).")
        }
        crate::compiler_frontend::compiler_messages::InvalidCallShapeReason::DuplicateArgument { .. } => {
            "An argument was provided more than once for a parameter.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidCallShapeReason::NamedArgumentNotFound { .. } => {
            "A named argument does not match any parameter of this function.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidCallShapeReason::PositionalAfterNamed => {
            "Positional arguments are not allowed after named arguments in a call.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidCallShapeReason::NamedArgumentsNotSupported => {
            "Named arguments are not supported for this call.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidCallShapeReason::MutableAccessRequired { .. } => {
            "A parameter requires mutable access (~), but it was not provided.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidCallShapeReason::MutableAccessNotAllowed { .. } => {
            "A parameter does not allow mutable access (~), but it was provided.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidCallShapeReason::MutableAccessOnNonPlace { .. } => {
            "Mutable access (~) requires a mutable place (variable or field), but a non-place expression was provided.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidCallShapeReason::MutableAccessOnImmutablePlace { .. } => {
            "Mutable access (~) requires a mutable place, but an immutable place was provided.".to_string()
        }
    }
}

pub(crate) fn invalid_return_shape_message(
    reason: crate::compiler_frontend::compiler_messages::InvalidReturnShapeReason,
) -> String {
    match reason {
        crate::compiler_frontend::compiler_messages::InvalidReturnShapeReason::BareReturnWithExpectedValues { expected_count } => {
            format!("Bare 'return' in a function that expects {expected_count} return value(s).")
        }
        crate::compiler_frontend::compiler_messages::InvalidReturnShapeReason::ReturnValuesWithBareSignature => {
            "This function has no return signature, so 'return' must be bare.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidReturnShapeReason::TooManyReturnValues { expected_count } => {
            format!("Return provides more values than the function signature expects ({expected_count}).")
        }
        crate::compiler_frontend::compiler_messages::InvalidReturnShapeReason::TooFewReturnValues { expected_count, provided_count } => {
            format!("Return provides {provided_count} value(s), but the function expects {expected_count}.")
        }
        crate::compiler_frontend::compiler_messages::InvalidReturnShapeReason::MissingReturnBangValue => {
            "return! requires an error value.".to_string()
        }
        crate::compiler_frontend::compiler_messages::InvalidReturnShapeReason::FunctionMayFallThrough => {
            "Function can fall through without returning a value.".to_string()
        }
    }
}

fn named_value_or_default(
    name: Option<StringId>,
    string_table: &StringTable,
    fallback: &'static str,
) -> String {
    name.map(|name| format!("'{}'", string_table.resolve(name)))
        .unwrap_or_else(|| fallback.to_string())
}

pub(crate) fn relative_display_path_from_root(scope: &Path, root: &Path) -> String {
    let normalized_scope = normalize_path(scope);
    let normalized_root = normalize_path(root);

    normalized_scope
        .strip_prefix(&normalized_root)
        .unwrap_or(&normalized_scope)
        .to_string_lossy()
        .to_string()
}

pub(crate) fn resolved_display_path(scope: &InternedPath, string_table: &StringTable) -> String {
    let source_file = resolve_source_file_path(scope, string_table);

    match std::env::current_dir() {
        Ok(dir) => relative_display_path_from_root(&source_file, &dir),
        Err(err) => {
            eprintln!(
                "Compiler failed to determine the current directory for diagnostic display. {err}"
            );
            source_file.to_string_lossy().to_string()
        }
    }
}

pub(crate) fn resolve_source_file_path(
    scope: &InternedPath,
    string_table: &StringTable,
) -> PathBuf {
    let mut source_file = normalize_path(&scope.to_path_buf(string_table));

    // Header diagnostics use a synthetic "file.bst/header_name.header" scope so the terminal and
    // dev-server error pages both need to strip that suffix back to the original source file.
    if source_file
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .is_some_and(|file_name| file_name.ends_with(".header"))
    {
        source_file = match source_file.parent() {
            Some(parent) => parent.to_path_buf(),
            None => source_file,
        };
    }

    match std::fs::canonicalize(&source_file) {
        Ok(canonical_path) => normalize_path(&canonical_path),
        Err(_) => source_file,
    }
}

pub(crate) fn display_line_number(raw_line: i32) -> i32 {
    raw_line.saturating_add(1).max(1)
}

pub(crate) fn display_column_number(raw_column: i32) -> i32 {
    raw_column.saturating_add(1).max(1)
}
