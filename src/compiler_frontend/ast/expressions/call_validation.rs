//! Shared call-argument normalization and validation.
//!
//! WHAT: resolves raw parsed arguments into slot-ordered call arguments and enforces the shared
//! rules for named targets, defaults, type compatibility, and explicit access mode.
//! WHY: function calls, struct constructors, receiver methods, and builtin members all need the
//! same argument policy even though they build different AST nodes afterward.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::call_argument::{
    CallAccessMode, CallArgument, CallPassingMode,
};
use crate::compiler_frontend::ast::expressions::constructor_views::{
    ConstructorField, ConstructorFieldAccessMode,
};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::place_access::{ast_node_is_mutable_place, ast_node_is_place};
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidCallShapeReason, TypeMismatchContext,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;

use crate::compiler_frontend::external_packages::{
    ExternalAccessKind, ExternalFunctionDef, ExternalParameter,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::type_coercion::compatibility::{
    TypeCompatibilityCache, TypeCompatibilityMode,
};
use crate::compiler_frontend::type_coercion::contextual::coerce_expression_to_declared_type;
use rustc_hash::FxHashMap;

pub(crate) enum CallValidationError {
    Diagnostic(Box<CompilerDiagnostic>),
    Infrastructure(Box<CompilerError>),
}

impl From<CompilerDiagnostic> for CallValidationError {
    fn from(diagnostic: CompilerDiagnostic) -> Self {
        CallValidationError::Diagnostic(Box::new(diagnostic))
    }
}

impl From<CompilerError> for CallValidationError {
    fn from(error: CompilerError) -> Self {
        CallValidationError::Infrastructure(Box::new(error))
    }
}

#[derive(Clone, Copy)]
pub(crate) enum CallSurfaceKind {
    Function,
    StructConstructor,
    ChoiceConstructor,
    ReceiverMethod,
    BuiltinMember,
    HostFunction,
}

#[derive(Clone, Copy)]
pub(crate) struct CallDiagnosticContext<'a> {
    pub kind: CallSurfaceKind,
    pub callee_name: &'a str,
}

impl<'a> CallDiagnosticContext<'a> {
    pub(crate) fn function(callee_name: &'a str) -> Self {
        Self {
            kind: CallSurfaceKind::Function,
            callee_name,
        }
    }

    pub(crate) fn struct_constructor(callee_name: &'a str) -> Self {
        Self {
            kind: CallSurfaceKind::StructConstructor,
            callee_name,
        }
    }

    pub(crate) fn choice_constructor(callee_name: &'a str) -> Self {
        Self {
            kind: CallSurfaceKind::ChoiceConstructor,
            callee_name,
        }
    }

    pub(crate) fn receiver_method(callee_name: &'a str) -> Self {
        Self {
            kind: CallSurfaceKind::ReceiverMethod,
            callee_name,
        }
    }

    pub(crate) fn builtin_member(callee_name: &'a str) -> Self {
        Self {
            kind: CallSurfaceKind::BuiltinMember,
            callee_name,
        }
    }

    pub(crate) fn host_function(callee_name: &'a str) -> Self {
        Self {
            kind: CallSurfaceKind::HostFunction,
            callee_name,
        }
    }

    fn callable_title(self) -> &'static str {
        match self.kind {
            CallSurfaceKind::Function => "Function",
            CallSurfaceKind::StructConstructor => "Struct constructor",
            CallSurfaceKind::ChoiceConstructor => "Choice constructor",
            CallSurfaceKind::ReceiverMethod => "Receiver method",
            CallSurfaceKind::BuiltinMember => "Builtin member",
            CallSurfaceKind::HostFunction => "Host function",
        }
    }
}

pub(crate) enum ExpectedAccessMode {
    Shared,
    Mutable,
}

pub(crate) enum ExpectedParameterType {
    Known(TypeId),
    UnknownExternal,
}

pub(crate) struct ParameterExpectation {
    pub name: Option<StringId>,
    /// Canonical type for the parameter, or `UnknownExternal` when the external
    /// package does not expose a resolved frontend type.
    pub expected_type: ExpectedParameterType,
    pub access_mode: ExpectedAccessMode,
    pub default_value: Option<Expression>,
}

enum CallTypeValidation<'a> {
    Validate(&'a mut TypeCompatibilityCache),
    Skip,
}

pub(crate) struct CallArgumentResolutionContext<'a> {
    pub(crate) string_table: &'a mut StringTable,
    pub(crate) type_environment: &'a TypeEnvironment,
    pub(crate) compatibility_cache: &'a mut TypeCompatibilityCache,
}

struct CallArgumentPolicyContext<'a> {
    string_table: &'a mut StringTable,
    type_environment: &'a TypeEnvironment,
    type_validation: CallTypeValidation<'a>,
}

/// Builds one expectation per user-defined parameter declaration.
pub(crate) fn expectations_from_user_parameters(
    parameters: &[Declaration],
) -> Vec<ParameterExpectation> {
    parameters
        .iter()
        .map(|parameter| ParameterExpectation {
            name: parameter.id.name(),
            expected_type: ExpectedParameterType::Known(parameter.value.type_id),
            access_mode: if parameter.value.value_mode.is_mutable() {
                ExpectedAccessMode::Mutable
            } else {
                ExpectedAccessMode::Shared
            },
            default_value: match parameter.value.kind {
                ExpressionKind::NoValue => None,
                _ => Some(parameter.value.clone()),
            },
        })
        .collect()
}

/// Maps a single external parameter into the frontend's `ParameterExpectation`.
///
/// WHAT: converts `ExternalSignatureType` and `ExternalAccessKind` into the canonical
///       `ExpectedParameterType` and `ExpectedAccessMode` used by call validation.
/// WHY: every external function, including package functions that operate on opaque handles,
///      uses the same argument validation path.
fn parameter_expectation_from_external(
    parameter: &ExternalParameter,
    type_environment: &mut TypeEnvironment,
) -> ParameterExpectation {
    let expected_type = match parameter
        .language_type
        .to_parameter_type_id(type_environment)
    {
        Some(type_id) => ExpectedParameterType::Known(type_id),
        None => ExpectedParameterType::UnknownExternal,
    };

    ParameterExpectation {
        name: None,
        expected_type,
        access_mode: match parameter.access_kind {
            ExternalAccessKind::Shared => ExpectedAccessMode::Shared,
            ExternalAccessKind::Mutable => ExpectedAccessMode::Mutable,
        },
        default_value: None,
    }
}

pub(crate) fn expectations_from_host_function(
    function: &ExternalFunctionDef,
    type_environment: &mut TypeEnvironment,
) -> Vec<ParameterExpectation> {
    function
        .parameters
        .iter()
        .map(|parameter| parameter_expectation_from_external(parameter, type_environment))
        .collect()
}

pub(crate) fn expectations_from_constructor_fields(
    fields: &[ConstructorField],
) -> Vec<ParameterExpectation> {
    fields
        .iter()
        .map(|field| ParameterExpectation {
            name: field.name.name(),
            expected_type: ExpectedParameterType::Known(field.type_id),
            access_mode: match field.access_mode {
                ConstructorFieldAccessMode::Shared => ExpectedAccessMode::Shared,
                ConstructorFieldAccessMode::Mutable => ExpectedAccessMode::Mutable,
            },
            default_value: field.default_value.clone(),
        })
        .collect()
}

pub(crate) fn expectations_from_receiver_method_signature(
    parameters_excluding_receiver: &[Declaration],
) -> Vec<ParameterExpectation> {
    expectations_from_user_parameters(parameters_excluding_receiver)
}

/// Resolves raw parsed call arguments into declaration-order slots.
///
/// WHAT: this is the shared normalization boundary for all call-shaped syntax.
/// WHY: once one caller changes argument policy, every other caller should inherit it from here.
pub(crate) fn resolve_call_arguments(
    diagnostics: CallDiagnosticContext<'_>,
    args: &[CallArgument],
    expectations: &[ParameterExpectation],
    location: SourceLocation,
    context: CallArgumentResolutionContext<'_>,
) -> Result<Vec<CallArgument>, CallValidationError> {
    resolve_call_arguments_with_type_policy(
        diagnostics,
        args,
        expectations,
        location,
        CallArgumentPolicyContext {
            string_table: context.string_table,
            type_environment: context.type_environment,
            type_validation: CallTypeValidation::Validate(context.compatibility_cache),
        },
    )
}

/// Resolves call arguments through shared shape/default/access rules without
/// running the ordinary final type-compatibility check.
///
/// WHAT: generic template validation first binds callee generic parameters to
/// caller generic `TypeId`s, then substitutes the call signature. At that point
/// exact `TypeId` equality is intentionally too strict for the pre-substitution
/// template parameters.
/// WHY: this keeps named arguments, arity, defaults, and mutable-access rules in
/// one shared owner while allowing generic-aware validation to supply its own
/// type evidence.
pub(crate) fn resolve_call_arguments_shape_and_access(
    diagnostics: CallDiagnosticContext<'_>,
    args: &[CallArgument],
    expectations: &[ParameterExpectation],
    location: SourceLocation,
    string_table: &mut StringTable,
    type_environment: &TypeEnvironment,
) -> Result<Vec<CallArgument>, CallValidationError> {
    resolve_call_arguments_with_type_policy(
        diagnostics,
        args,
        expectations,
        location,
        CallArgumentPolicyContext {
            string_table,
            type_environment,
            type_validation: CallTypeValidation::Skip,
        },
    )
}

fn resolve_call_arguments_with_type_policy(
    diagnostics: CallDiagnosticContext<'_>,
    args: &[CallArgument],
    expectations: &[ParameterExpectation],
    location: SourceLocation,
    context: CallArgumentPolicyContext<'_>,
) -> Result<Vec<CallArgument>, CallValidationError> {
    let CallArgumentPolicyContext {
        string_table,
        type_environment,
        mut type_validation,
    } = context;

    // Validation flow order is intentionally fixed:
    // 1) build parameter expectation table,
    // 2) resolve named targets,
    // 3) enforce positional-before-named ordering,
    // 4) detect duplicate targets,
    // 5) fill defaults,
    // 6) detect missing required parameters,
    // 7) validate types,
    // 8) validate access mode.
    let mut resolved = resolve_call_argument_slots_typed(
        diagnostics,
        args,
        expectations,
        location.clone(),
        string_table,
    )?;

    // ------------------------
    //  Fill default values
    // ------------------------
    for (slot, expectation) in expectations.iter().enumerate() {
        if resolved[slot].is_none() {
            if let Some(default_value) = &expectation.default_value {
                let defaulted = default_value.clone();
                debug_assert!(
                    type_environment.get(defaulted.type_id).is_some(),
                    "default argument expression carried orphan TypeId({}) not registered in the active TypeEnvironment",
                    defaulted.type_id.0,
                );
                resolved[slot] = Some(CallArgument::positional(
                    defaulted,
                    CallAccessMode::Shared,
                    location.clone(),
                ));
            } else {
                return Err(CompilerDiagnostic::invalid_call_shape(
                    InvalidCallShapeReason::MissingArgument {
                        parameter_name: expectation.name,
                        parameter_index: slot,
                    },
                    None,
                    location.clone(),
                )
                .into());
            }
        }
    }

    // ------------------------
    //  Validate and coerce each argument
    // ------------------------
    let mut ordered = Vec::with_capacity(expectations.len());
    for (slot, expectation) in expectations.iter().enumerate() {
        let Some(argument) = resolved[slot].take() else {
            let message = format!(
                "Call argument resolution left required slot {} empty for {} '{}'",
                slot + 1,
                diagnostics.callable_title(),
                diagnostics.callee_name
            );
            return Err(CompilerError::compiler_error(message).into());
        };

        let passing_mode = classify_call_passing_mode(
            &diagnostics,
            &argument,
            expectation,
            slot,
            location.clone(),
            string_table,
        )?;

        let expected_type_id = match expectation.expected_type {
            ExpectedParameterType::Known(type_id) => type_id,
            ExpectedParameterType::UnknownExternal => {
                // Unknown external types skip compatibility checking.
                ordered.push(argument.with_passing_mode(passing_mode));
                continue;
            }
        };
        let actual_type_id = argument.value.type_id;

        if let CallTypeValidation::Validate(compatibility_cache) = &mut type_validation
            && !is_call_argument_type_compatible(
                expectation,
                actual_type_id,
                passing_mode,
                type_environment,
                compatibility_cache,
            )
        {
            let mismatch_context = match diagnostics.kind {
                CallSurfaceKind::StructConstructor | CallSurfaceKind::ChoiceConstructor => {
                    TypeMismatchContext::ConstructorArgument
                }
                CallSurfaceKind::ReceiverMethod | CallSurfaceKind::BuiltinMember => {
                    TypeMismatchContext::ReceiverArgument
                }
                _ => TypeMismatchContext::FunctionArgument,
            };
            return Err(CompilerDiagnostic::type_mismatch(
                expected_type_id,
                actual_type_id,
                mismatch_context,
                argument.location.clone(),
            )
            .into());
        }

        let mut normalized_argument = argument.with_passing_mode(passing_mode);
        normalized_argument.value = coerce_expression_to_declared_type(
            normalized_argument.value,
            expected_type_id,
            type_environment,
        );

        ordered.push(normalized_argument);
    }

    Ok(ordered)
}

/// Resolves raw call arguments into declaration-order slots without filling defaults or
/// validating types.
///
/// WHAT: generic constructor inference needs the same named/positional routing as full call
/// validation, but omitted defaulted fields must not infer type parameters.
pub(crate) fn resolve_call_argument_slots_typed(
    diagnostics: CallDiagnosticContext<'_>,
    args: &[CallArgument],
    expectations: &[ParameterExpectation],
    location: SourceLocation,
    string_table: &mut StringTable,
) -> Result<Vec<Option<CallArgument>>, CallValidationError> {
    let mut resolved: Vec<Option<CallArgument>> = vec![None; expectations.len()];
    let mut positional_cursor = 0usize;
    let mut saw_named_argument = false;
    let mut parameter_name_to_slot: FxHashMap<StringId, usize> = FxHashMap::default();

    // ------------------------
    //  Build parameter name index
    // ------------------------
    for (slot_index, expectation) in expectations.iter().enumerate() {
        if let Some(name) = expectation.name {
            parameter_name_to_slot.insert(name, slot_index);
        }
    }

    // ------------------------
    //  Route each argument to its slot
    // ------------------------
    for argument in args {
        let slot = if let Some(target_name) = argument.target_param {
            saw_named_argument = true;
            let Some(slot) = parameter_name_to_slot.get(&target_name).copied() else {
                let known_parameters: Vec<StringId> = expectations
                    .iter()
                    .filter_map(|expectation| expectation.name)
                    .collect();
                return Err(CompilerDiagnostic::invalid_call_shape(
                    InvalidCallShapeReason::NamedArgumentNotFound {
                        name: target_name,
                        known_parameters,
                    },
                    Some(string_table.intern(diagnostics.callee_name)),
                    argument
                        .target_location
                        .clone()
                        .unwrap_or_else(|| argument.location.clone()),
                )
                .into());
            };
            slot
        } else {
            if saw_named_argument {
                return Err(CompilerDiagnostic::invalid_call_shape(
                    InvalidCallShapeReason::PositionalAfterNamed,
                    Some(string_table.intern(diagnostics.callee_name)),
                    argument.location.clone(),
                )
                .into());
            }

            while positional_cursor < expectations.len() && resolved[positional_cursor].is_some() {
                positional_cursor += 1;
            }
            if positional_cursor >= expectations.len() {
                return Err(CompilerDiagnostic::invalid_call_shape(
                    InvalidCallShapeReason::ExtraPositionalArgument {
                        expected_count: expectations.len(),
                    },
                    None,
                    location.clone(),
                )
                .into());
            }
            let slot = positional_cursor;
            positional_cursor += 1;
            slot
        };

        if resolved[slot].is_some() {
            return Err(CompilerDiagnostic::invalid_call_shape(
                InvalidCallShapeReason::DuplicateArgument {
                    parameter_name: expectations[slot].name,
                    parameter_index: slot,
                },
                Some(string_table.intern(diagnostics.callee_name)),
                argument
                    .target_location
                    .clone()
                    .unwrap_or_else(|| argument.location.clone()),
            )
            .into());
        }

        resolved[slot] = Some(argument.clone());
    }

    Ok(resolved)
}

fn classify_call_passing_mode(
    diagnostics: &CallDiagnosticContext<'_>,
    argument: &CallArgument,
    expectation: &ParameterExpectation,
    slot_index: usize,
    _location: SourceLocation,
    string_table: &mut StringTable,
) -> Result<CallPassingMode, CallValidationError> {
    match (argument.access_mode, &expectation.access_mode) {
        // Shared argument passed to a shared parameter: no restriction.
        (CallAccessMode::Shared, ExpectedAccessMode::Shared) => Ok(CallPassingMode::Shared),

        // Shared argument passed to a mutable parameter: allowed only for non-place expressions.
        // Fresh rvalues can be materialized for mutable calls; existing places require `~`.
        (CallAccessMode::Shared, ExpectedAccessMode::Mutable) => {
            if !expression_is_place(&argument.value) {
                return Ok(CallPassingMode::FreshMutableValue);
            }
            Err(CompilerDiagnostic::invalid_call_shape(
                InvalidCallShapeReason::MutableAccessRequired {
                    parameter_name: expectation.name,
                    parameter_index: slot_index,
                },
                Some(string_table.intern(diagnostics.callee_name)),
                argument.location.clone(),
            )
            .into())
        }

        // Mutable argument passed to a shared parameter: never allowed.
        (CallAccessMode::Mutable, ExpectedAccessMode::Shared) => {
            Err(CompilerDiagnostic::invalid_call_shape(
                InvalidCallShapeReason::MutableAccessNotAllowed {
                    parameter_name: expectation.name,
                    parameter_index: slot_index,
                },
                Some(string_table.intern(diagnostics.callee_name)),
                argument.location.clone(),
            )
            .into())
        }

        // Mutable argument passed to a mutable parameter: allowed only for mutable places.
        (CallAccessMode::Mutable, ExpectedAccessMode::Mutable) => {
            if !expression_is_place(&argument.value) {
                return Err(CompilerDiagnostic::invalid_call_shape(
                    InvalidCallShapeReason::MutableAccessOnNonPlace {
                        parameter_name: expectation.name,
                        parameter_index: slot_index,
                    },
                    Some(string_table.intern(diagnostics.callee_name)),
                    argument.location.clone(),
                )
                .into());
            }
            if !expression_is_mutable_place(&argument.value) {
                return Err(CompilerDiagnostic::invalid_call_shape(
                    InvalidCallShapeReason::MutableAccessOnImmutablePlace {
                        parameter_name: expectation.name,
                        parameter_index: slot_index,
                    },
                    Some(string_table.intern(diagnostics.callee_name)),
                    argument.location.clone(),
                )
                .into());
            }
            Ok(CallPassingMode::MutablePlace)
        }
    }
}

fn is_call_argument_type_compatible(
    expectation: &ParameterExpectation,
    actual_type_id: TypeId,
    passing_mode: CallPassingMode,
    type_environment: &TypeEnvironment,
    compatibility_cache: &mut TypeCompatibilityCache,
) -> bool {
    let expected_type_id = match expectation.expected_type {
        ExpectedParameterType::Known(type_id) => type_id,
        ExpectedParameterType::UnknownExternal => {
            // Unknown expected type (e.g. external Handle/Void): skip compatibility check.
            return true;
        }
    };

    let compatibility_mode = if passing_mode == CallPassingMode::FreshMutableValue {
        TypeCompatibilityMode::FreshMutableRvalue
    } else {
        TypeCompatibilityMode::Standard
    };

    compatibility_cache.is_compatible(
        expected_type_id,
        actual_type_id,
        compatibility_mode,
        type_environment,
    )
}

/// Returns `true` if the expression can be used as a place (an addressable memory location).
///
/// WHAT: references and single-node runtime expressions are places.
/// WHY: call validation needs to distinguish place expressions from rvalues when enforcing
///      mutable-access requirements.
fn expression_is_place(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Reference(_) => true,
        ExpressionKind::Runtime(nodes) if nodes.len() == 1 => ast_node_is_place(&nodes[0]),
        _ => false,
    }
}

/// Returns `true` if the expression is a mutable place.
///
/// WHAT: extends `expression_is_place` with a mutability check on the expression's value mode
///       or the underlying AST node.
/// WHY: mutable parameter passing requires the argument to be a mutable place, not just any place.
fn expression_is_mutable_place(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Reference(_) => expression.value_mode.is_mutable(),
        ExpressionKind::Runtime(nodes) if nodes.len() == 1 => ast_node_is_mutable_place(&nodes[0]),
        _ => false,
    }
}
