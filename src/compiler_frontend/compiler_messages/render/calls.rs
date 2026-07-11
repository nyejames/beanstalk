//! Call and return diagnostic text renderers.
//!
//! WHAT: renders arity, argument-shape, mutability-at-call, and return-shape diagnostics.
//! WHY: call/return validation is a distinct frontend boundary with its own payload family.

use super::{
    DiagnosticRenderContext, closest_name_suggestion, diagnostic_type_name, named_value_or_default,
};
use crate::compiler_frontend::compiler_messages::{
    InvalidAssignmentTargetReason, InvalidBuiltinCallReason, InvalidCallShapeReason,
    InvalidCastReason, InvalidCopyTargetReason, InvalidFieldAccessReason, InvalidMultiBindReason,
    InvalidReceiverCallReason, InvalidReturnShapeReason,
};
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};

/// Build a "call context" prefix that names the function being called when available.
///
/// WHAT: many call-shape diagnostics carry the callee name as an optional payload field.
/// WHY: including the function name makes the message immediately actionable instead of
/// leaving the user to cross-reference the source location.
fn call_prefix(callee_name: Option<StringId>, string_table: &StringTable) -> String {
    callee_name
        .map(|name| format!("Call to '{}'", string_table.resolve(name)))
        .unwrap_or_else(|| "Call".to_owned())
}

/// Resolve the parameter name for display, falling back to a 1-based position label.
fn parameter_label(
    parameter_name: Option<StringId>,
    parameter_index: usize,
    string_table: &StringTable,
) -> String {
    parameter_name
        .map(|name| format!("parameter '{}'", string_table.resolve(name)))
        .unwrap_or_else(|| {
            format!(
                "parameter {parameter_index} (1-based: #{})",
                parameter_index + 1
            )
        })
}

pub(crate) fn invalid_call_shape_message(
    reason: InvalidCallShapeReason,
    callee_name: Option<StringId>,
    string_table: &StringTable,
) -> String {
    let prefix = call_prefix(callee_name, string_table);

    match reason {
        InvalidCallShapeReason::MissingArgument {
            parameter_name,
            parameter_index,
        } => {
            let label = parameter_label(parameter_name, parameter_index, string_table);
            format!("{prefix} is missing an argument for {label}.")
        }
        InvalidCallShapeReason::ExtraPositionalArgument { expected_count } => {
            format!("{prefix} provides more positional arguments than expected (expected {expected_count}).")
        }
        InvalidCallShapeReason::DuplicateArgument {
            parameter_name,
            parameter_index,
        } => {
            let label = parameter_label(parameter_name, parameter_index, string_table);
            format!("{prefix} provides an argument for {label} more than once. Each parameter may only be supplied once.")
        }
        InvalidCallShapeReason::NamedArgumentNotFound {
            name,
            known_parameters,
       } => {
            let arg_name = string_table.resolve(name);
            let suggestion = closest_name_suggestion(arg_name, &known_parameters, string_table);
            let known = known_parameters
               .iter()
                .map(|p| string_table.resolve(*p))
                .collect::<Vec<_>>()
                .join(", ");
            match suggestion {
                Some(suggested) => {
                    format!(
                        "{prefix} has named argument '{arg_name}' which does not match any parameter. Did you mean '{suggested}'? Known parameters: [{known}]."
                    )
                }
                None => {
                    format!(
                        "{prefix} has named argument '{arg_name}' which does not match any parameter. Known parameters: [{known}]."
                    )
                }
            }
        }
        InvalidCallShapeReason::PositionalAfterNamed => {
            "Positional arguments must come before named arguments. Reorder the call so all positional arguments come first.".to_string()
        }
        InvalidCallShapeReason::NamedArgumentsNotSupported => {
            "Named arguments are not supported for this call. Use positional arguments only.".to_string()
        }
        InvalidCallShapeReason::MutableAccessRequired {
            parameter_name,
            parameter_index,
        } => {
            let label = parameter_label(parameter_name, parameter_index, string_table);
            format!("{prefix} requires mutable access (~) for {label}, but it was not provided. Prefix an existing mutable place with ~, for example `~value`.")
        }
        InvalidCallShapeReason::MutableAccessNotAllowed {
            parameter_name,
            parameter_index,
        } => {
            let label = parameter_label(parameter_name, parameter_index, string_table);
            format!("{prefix} does not allow mutable access (~) for {label}, but it was provided. Remove the ~ prefix from this argument.")
        }
        InvalidCallShapeReason::MutableAccessOnNonPlace {
            parameter_name,
            parameter_index,
        } => {
            let label = parameter_label(parameter_name, parameter_index, string_table);
            format!("{prefix} requires mutable access (~) for {label}, but a non-place expression was provided. Mutable access needs a variable or field, not a literal or computed value.")
        }
        InvalidCallShapeReason::MutableAccessOnImmutablePlace {
            parameter_name,
            parameter_index,
        } => {
            let label = parameter_label(parameter_name, parameter_index, string_table);
            format!("{prefix} requires mutable access (~) for {label}, but an immutable place was provided. Declare the binding with ~ to allow mutation.")
        }
        InvalidCallShapeReason::ReactiveSourceRequired {
            parameter_name,
            parameter_index,
        } => {
            let label = parameter_label(parameter_name, parameter_index, string_table);
            format!("{prefix} requires an existing reactive source for {label}. Pass a value declared with `$Type` or `$=` instead of an ordinary value.")
        }
    }
}

pub(crate) fn invalid_return_shape_message(reason: InvalidReturnShapeReason) -> String {
    match reason {
        InvalidReturnShapeReason::BareReturnWithExpectedValues { expected_count } => {
            format!("Bare 'return' in a function that expects {expected_count} return value(s).")
        }
        InvalidReturnShapeReason::ReturnValuesWithBareSignature => {
            "This function has no return signature, so 'return' must be bare.".to_string()
        }
        InvalidReturnShapeReason::TooManyReturnValues { expected_count } => {
            format!(
                "Return provides more values than the function signature expects ({expected_count})."
            )
        }
        InvalidReturnShapeReason::TooFewReturnValues {
            expected_count,
            provided_count,
        } => {
            format!(
                "Return provides {provided_count} value(s), but the function expects {expected_count}."
            )
        }
        InvalidReturnShapeReason::MissingReturnBangValue => {
            "return! requires an error value.".to_string()
        }
        InvalidReturnShapeReason::FunctionMayFallThrough => {
            "This function can fall through without returning on every path. Add a return, a terminal else branch, or an `assert(false)` guard to ensure all reachable paths produce a value.".to_string()
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
        InvalidAssignmentTargetReason::MapIndexedWriteRemoved => {
            "Indexed assignment through map `get(...)` has been removed. Use `~map.set(key, value)!` or handle `~map.set(key, value) catch:` instead.".to_string()
        }
        InvalidAssignmentTargetReason::MapPropertyWriteRemoved => {
            "Map `length` is a read-only property and cannot be assigned.".to_string()
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
        InvalidBuiltinCallReason::MapLengthIsProperty => {
            "Map `length` is a property, not a method. Use `map.length` without parentheses."
                .to_string()
        }
        InvalidBuiltinCallReason::ScalarConstructorRemoved => {
            format!(
                "{builtin_text}(...) constructor-style conversions are removed. Use `cast` instead."
            )
        }
    }
}

pub(crate) fn invalid_cast_message(
    reason: InvalidCastReason,
    source_type: Option<TypeId>,
    target_type: Option<TypeId>,
    context: DiagnosticRenderContext<'_>,
) -> String {
    let source = source_type
        .map(|type_id| diagnostic_type_name(type_id, context))
        .unwrap_or_else(|| "the source value".to_owned());
    let target = target_type
        .map(|type_id| diagnostic_type_name(type_id, context))
        .unwrap_or_else(|| "the target type".to_owned());

    match reason {
        InvalidCastReason::MissingExplicitTarget => {
            "`cast` requires an explicit builtin target type at the receiving boundary.".to_owned()
        }
        InvalidCastReason::TargetNotBuiltin => {
            format!("`cast` targets must be compiler-supported builtin types; found '{target}'.")
        }
        InvalidCastReason::TargetIsGenericParameter => {
            format!("`cast` cannot infer a generic target such as '{target}'.")
        }
        InvalidCastReason::SameSourceAndTarget => {
            format!("This value already has type '{target}'. Remove `cast`.")
        }
        InvalidCastReason::SourceIsOptional => {
            format!("`cast` does not automatically unwrap optional source values; found '{source}'.")
        }
        InvalidCastReason::OperandIsFallible => {
            "`cast` only handles cast failures. Handle the operand's `Error!` result before casting."
                .to_owned()
        }
        InvalidCastReason::OperandArityMismatch => {
            "`cast` converts exactly one source value. Cast each result slot separately.".to_owned()
        }
        InvalidCastReason::TargetArityMismatch => {
            "`cast` requires exactly one target value. Cast each target slot separately.".to_owned()
        }
        InvalidCastReason::FallibleEvidenceRequiresHandling => {
            "`cast` selected fallible evidence. Use `cast!` or `cast ... catch:`.".to_owned()
        }
        InvalidCastReason::InfallibleEvidenceCannotUseFallibleForm => {
            "`cast!` and `cast ... catch:` are only valid for fallible casts.".to_owned()
        }
        InvalidCastReason::PropagationRequiresErrorReturn => {
            "`cast!` requires the current function to have an `Error!` return slot.".to_owned()
        }
        InvalidCastReason::PropagationAndRecoveryConflict => {
            "`cast!` cannot also use `catch:`. Choose propagation or local recovery.".to_owned()
        }
        InvalidCastReason::BangMustAttachToCast => {
            "The `!` must be attached to `cast` as `cast!`.".to_owned()
        }
        InvalidCastReason::ScalarConstructorRemoved => {
            "Constructor-style scalar conversions are removed. Use `cast` at an explicit typed boundary."
                .to_owned()
        }
        InvalidCastReason::NoEvidence => {
            format!("No cast evidence exists from '{source}' to '{target}'.")
        }
        InvalidCastReason::BuiltinEvidenceNotConstFoldable => {
            "This builtin cast cannot be used in const-required contexts yet.".to_owned()
        }
        InvalidCastReason::UserDefinedEvidenceNotConstFoldable => {
            "User-defined cast evidence cannot be used in const-required contexts.".to_owned()
        }
        InvalidCastReason::GenericBoundEvidenceNotConstFoldable => {
            "Generic-bound cast evidence cannot be used in const-required contexts.".to_owned()
        }
        InvalidCastReason::BuiltinCastFailedInConst => {
            "This builtin cast failed while evaluating a const-required expression.".to_owned()
        }
        InvalidCastReason::CatchHandlerNotConstFoldable => {
            "The `catch` handler for this cast must be fully foldable in a const-required context."
                .to_owned()
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
        InvalidReceiverCallReason::MutableMapRequired => {
            format!(
                "Map mutating method {method_text} requires a mutable map receiver."
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
        InvalidReceiverCallReason::AmbiguousGenericBoundMethod => {
            format!(
                "{method_text} is provided by more than one generic bound for {receiver_text}. Add a more specific bound or rename one of the trait requirements."
            )
        }
    }
}

/// Build a "did you mean?" suggestion for a misspelled field name.
///
/// WHAT: uses the same edit-distance heuristic as named-argument suggestions.
/// WHY: typos in field access are one of the most common mistakes; suggesting the
/// correct field name is immediately actionable.
fn field_suggestion(
    field_name: Option<StringId>,
    known_fields: &[StringId],
    string_table: &StringTable,
) -> Option<String> {
    let name = field_name?;
    let name_str = string_table.resolve(name);
    closest_name_suggestion(name_str, known_fields, string_table)
}

/// Build a hint listing available fields when no close suggestion is found.
fn available_fields_hint(known_fields: &[StringId], string_table: &StringTable) -> String {
    if known_fields.is_empty() {
        return String::new();
    }
    let names = known_fields
        .iter()
        .map(|f| string_table.resolve(*f))
        .collect::<Vec<_>>()
        .join(", ");
    format!(" Available: [{names}]")
}

pub(crate) fn invalid_copy_target_message(reason: InvalidCopyTargetReason) -> String {
    match reason {
        InvalidCopyTargetReason::FunctionValue => {
            "The 'copy' keyword only accepts places, not function values or calls. Assign the result to a variable first, then copy that variable.".to_string()
        }
        InvalidCopyTargetReason::NonPlace => {
            "The 'copy' keyword requires a variable or field, not a literal or computed expression. Assign the value to a variable first, then copy it, for example `tmp = value` followed by `copy tmp`.".to_string()
        }
    }
}

pub(crate) fn invalid_field_access_message(
    reason: InvalidFieldAccessReason,
    field_name: Option<StringId>,
    receiver_type: Option<TypeId>,
    known_fields: &[StringId],
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
        InvalidFieldAccessReason::UnknownMember => {
            let suggestion = field_suggestion(field_name, known_fields, string_table);
            match receiver_text {
                Some(receiver_text) => match suggestion {
                    Some(suggested) => {
                        format!("Property or method {field_text} not found for '{receiver_text}'. Did you mean '{suggested}'?")
                    }
                    None => {
                        let available = available_fields_hint(known_fields, string_table);
                        format!("Property or method {field_text} not found for '{receiver_text}'.{available}")
                    }
                },
                None => format!("Property or method {field_text} not found."),
            }
        }
    }
}
