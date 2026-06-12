//! Call and return diagnostic text renderers.
//!
//! WHAT: renders arity, argument-shape, mutability-at-call, and return-shape diagnostics.
//! WHY: call/return validation is a distinct frontend boundary with its own payload family.

use super::{DiagnosticRenderContext, diagnostic_type_name, named_value_or_default};
use crate::compiler_frontend::compiler_messages::{
    InvalidAssignmentTargetReason, InvalidBuiltinCallReason, InvalidCallShapeReason,
    InvalidCopyTargetReason, InvalidFieldAccessReason, InvalidMultiBindReason,
    InvalidReceiverCallReason, InvalidReturnShapeReason,
};
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};

pub(crate) fn invalid_call_shape_message(reason: InvalidCallShapeReason) -> String {
    match reason {
        InvalidCallShapeReason::MissingArgument { .. } => {
            "Missing argument for a parameter in this call.".to_string()
        }
        InvalidCallShapeReason::ExtraPositionalArgument { expected_count } => {
            format!("This call provides more than {expected_count} positional argument(s).")
        }
        InvalidCallShapeReason::DuplicateArgument { .. } => {
            "An argument was provided more than once for a parameter.".to_string()
        }
        InvalidCallShapeReason::NamedArgumentNotFound { .. } => {
            "A named argument does not match any parameter of this function.".to_string()
        }
        InvalidCallShapeReason::PositionalAfterNamed => {
            "Positional arguments are not allowed after named arguments in a call.".to_string()
        }
        InvalidCallShapeReason::NamedArgumentsNotSupported => {
            "Named arguments are not supported for this call.".to_string()
        }
        InvalidCallShapeReason::MutableAccessRequired { .. } => {
            "A parameter requires mutable access (~), but it was not provided.".to_string()
        }
        InvalidCallShapeReason::MutableAccessNotAllowed { .. } => {
            "A parameter does not allow mutable access (~), but it was provided.".to_string()
        }
        InvalidCallShapeReason::MutableAccessOnNonPlace { .. } => {
            "Mutable access (~) requires a mutable place (variable or field), but a non-place expression was provided.".to_string()
        }
        InvalidCallShapeReason::MutableAccessOnImmutablePlace { .. } => {
            "Mutable access (~) requires a mutable place, but an immutable place was provided.".to_string()
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
            "Function can fall through without returning a value.".to_string()
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
        InvalidReceiverCallReason::AmbiguousTraitEvidenceMethod => {
            format!(
                "{method_text} is provided by more than one visible trait evidence surface for {receiver_text}. Import a narrower trait surface or call an ordinary receiver method with an unambiguous name."
            )
        }
    }
}

pub(crate) fn invalid_copy_target_message(reason: InvalidCopyTargetReason) -> String {
    match reason {
        InvalidCopyTargetReason::FunctionValue => {
            "The 'copy' keyword only accepts places, not function values or calls.".to_string()
        }
        InvalidCopyTargetReason::NonPlace => {
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
