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
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::place_access::{ast_node_is_mutable_place, ast_node_is_place};
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::external_packages::{ExternalAccessKind, ExternalFunctionDef};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::type_coercion::compatibility::is_type_compatible;
use crate::compiler_frontend::type_coercion::diagnostics::{
    argument_conversion_hint, expected_found_clause, offending_value_clause,
};
use crate::return_compiler_error;
use crate::return_rule_error;
use crate::return_type_error;
use rustc_hash::FxHashMap;

#[derive(Clone, Copy)]
pub(crate) enum CallSurfaceKind {
    Function,
    StructConstructor,
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
            CallSurfaceKind::ReceiverMethod => "Receiver method",
            CallSurfaceKind::BuiltinMember => "Builtin member",
            CallSurfaceKind::HostFunction => "Host function",
        }
    }

    fn slot_noun(self) -> &'static str {
        match self.kind {
            CallSurfaceKind::StructConstructor => "field",
            _ => "parameter",
        }
    }

    fn known_slots_label(self) -> &'static str {
        match self.kind {
            CallSurfaceKind::StructConstructor => "Known fields",
            _ => "Known parameters",
        }
    }

    fn slot_noun_title(self) -> &'static str {
        match self.kind {
            CallSurfaceKind::StructConstructor => "Field",
            _ => "Parameter",
        }
    }

    fn primary_conversion_suggestion(self) -> &'static str {
        match self.kind {
            CallSurfaceKind::StructConstructor => {
                "Convert this field value to the declared struct field type"
            }
            _ => "Convert the argument to the expected type",
        }
    }

    fn callable_label(self) -> String {
        format!("{} '{}'", self.callable_title(), self.callee_name)
    }

    fn slot_label(
        self,
        expectation: &ParameterExpectation,
        slot: usize,
        string_table: &StringTable,
    ) -> String {
        expectation
            .name
            .map(|name| format!("'{}'", string_table.resolve(name)))
            .unwrap_or_else(|| format!("#{}", slot + 1))
    }
}

pub(crate) enum ExpectedAccessMode {
    Shared,
    Mutable,
}

pub(crate) struct ParameterExpectation {
    pub name: Option<StringId>,
    pub data_type: DataType,
    pub access_mode: ExpectedAccessMode,
    pub default_value: Option<Expression>,
}

/// Builds one expectation per user-defined parameter declaration.
pub(crate) fn expectations_from_user_parameters(
    parameters: &[Declaration],
) -> Vec<ParameterExpectation> {
    parameters
        .iter()
        .map(|parameter| ParameterExpectation {
            name: parameter.id.name(),
            data_type: parameter.value.data_type.clone(),
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

pub(crate) fn expectations_from_host_function(
    function: &ExternalFunctionDef,
) -> Vec<ParameterExpectation> {
    function
        .parameters
        .iter()
        .map(|parameter| ParameterExpectation {
            name: None,
            data_type: parameter
                .language_type
                .to_datatype()
                .unwrap_or(DataType::Inferred),
            access_mode: match parameter.access_kind {
                ExternalAccessKind::Shared => ExpectedAccessMode::Shared,
                ExternalAccessKind::Mutable => ExpectedAccessMode::Mutable,
            },
            default_value: None,
        })
        .collect()
}

pub(crate) fn expectations_from_struct_fields(fields: &[Declaration]) -> Vec<ParameterExpectation> {
    fields
        .iter()
        .map(|field| ParameterExpectation {
            name: field.id.name(),
            data_type: field.value.data_type.clone(),
            // Constructor arguments initialize a fresh value, so field mutability becomes
            // relevant only after construction, not at the call site.
            access_mode: ExpectedAccessMode::Shared,
            default_value: match field.value.kind {
                ExpressionKind::NoValue => None,
                _ => Some(field.value.clone()),
            },
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
    string_table: &StringTable,
) -> Result<Vec<CallArgument>, CompilerError> {
    // Validation flow order is intentionally fixed:
    // 1) build parameter expectation table,
    // 2) resolve named targets,
    // 3) enforce positional-before-named ordering,
    // 4) detect duplicate targets,
    // 5) fill defaults,
    // 6) detect missing required parameters,
    // 7) validate types,
    // 8) validate access mode.
    let mut resolved: Vec<Option<CallArgument>> = vec![None; expectations.len()];
    let mut positional_cursor = 0usize;
    let mut saw_named_argument = false;
    let mut name_to_slot: FxHashMap<StringId, usize> = FxHashMap::default();

    for (index, expectation) in expectations.iter().enumerate() {
        if let Some(name) = expectation.name {
            name_to_slot.insert(name, index);
        }
    }

    for argument in args {
        let slot = if let Some(target_name) = argument.target_param {
            saw_named_argument = true;
            let Some(slot) = name_to_slot.get(&target_name).copied() else {
                let known_parameters = expectations
                    .iter()
                    .filter_map(|expectation| expectation.name)
                    .map(|name| format!("'{}'", string_table.resolve(name)))
                    .collect::<Vec<_>>();
                let known_parameter_hint = if known_parameters.is_empty() {
                    String::from("This call accepts positional-only arguments.")
                } else {
                    format!(
                        "{}: {}",
                        diagnostics.known_slots_label(),
                        known_parameters.join(", ")
                    )
                };
                return_rule_error!(
                    format!(
                        "{} has no {} named '{}'. {}",
                        diagnostics.callable_label(),
                        diagnostics.slot_noun(),
                        string_table.resolve(target_name),
                        known_parameter_hint
                    ),
                    argument
                        .target_location
                        .clone()
                        .unwrap_or_else(|| argument.location.clone()),
                    {
                        CompilationStage => "Function Call Validation",
                        PrimarySuggestion => format!(
                            "Use a declared {} name in this call",
                            diagnostics.slot_noun()
                        ),
                    }
                );
            };
            slot
        } else {
            if saw_named_argument {
                return_rule_error!(
                    format!(
                        "{} does not allow positional arguments after named arguments",
                        diagnostics.callable_label()
                    ),
                    argument.location.clone(),
                    {
                        CompilationStage => "Function Call Validation",
                        PrimarySuggestion => "Move positional arguments before named arguments",
                    }
                );
            }

            while positional_cursor < expectations.len() && resolved[positional_cursor].is_some() {
                positional_cursor += 1;
            }
            if positional_cursor >= expectations.len() {
                return_type_error!(
                    format!(
                        "{} expects {} argument(s), but extra positional arguments were provided",
                        diagnostics.callable_label(),
                        expectations.len()
                    ),
                    location.clone(),
                    {
                        CompilationStage => "Function Call Validation",
                        PrimarySuggestion => "Remove extra arguments or update the function signature",
                    }
                );
            }
            let slot = positional_cursor;
            positional_cursor += 1;
            slot
        };

        if resolved[slot].is_some() {
            let slot_label = diagnostics.slot_label(&expectations[slot], slot, string_table);
            return_rule_error!(
                format!(
                    "{} {} was provided more than once",
                    diagnostics.slot_noun_title(),
                    slot_label
                ),
                argument
                    .target_location
                    .clone()
                    .unwrap_or_else(|| argument.location.clone()),
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => format!(
                        "Provide each {} at most once",
                        diagnostics.slot_noun()
                    ),
                }
            );
        }

        resolved[slot] = Some(argument.clone());
    }

    for (slot, expectation) in expectations.iter().enumerate() {
        if resolved[slot].is_none() {
            if let Some(default_value) = &expectation.default_value {
                resolved[slot] = Some(CallArgument::positional(
                    default_value.clone(),
                    CallAccessMode::Shared,
                    location.clone(),
                ));
            } else {
                let parameter_label = expectation
                    .name
                    .map(|name| format!("'{}'", string_table.resolve(name)))
                    .unwrap_or_else(|| format!("#{}", slot + 1));
                return_type_error!(
                    format!(
                        "Missing required argument for {} {} in {}",
                        diagnostics.slot_noun(),
                        parameter_label,
                        diagnostics.callable_label()
                    ),
                    location.clone(),
                    {
                        CompilationStage => "Function Call Validation",
                        PrimarySuggestion => format!(
                            "Provide values for all required {}s",
                            diagnostics.slot_noun()
                        ),
                    }
                );
            }
        }
    }

    let mut ordered = Vec::with_capacity(expectations.len());
    for (slot, expectation) in expectations.iter().enumerate() {
        let Some(argument) = resolved[slot].take() else {
            return_compiler_error!(
                format!(
                    "Call argument resolution left required slot {} empty for {}",
                    slot + 1,
                    diagnostics.callable_label()
                );
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => "This indicates a compiler bug in argument slot resolution. Please report this issue.",
                }
            );
        };

        let passing_mode = classify_call_passing_mode(
            &diagnostics,
            &argument,
            expectation,
            location.clone(),
            string_table,
        )?;

        if !is_call_argument_type_compatible(expectation, &argument, passing_mode) {
            let conversion_hint =
                argument_conversion_hint(&expectation.data_type, &argument.value.data_type);
            let slot_label = diagnostics.slot_label(expectation, slot, string_table);
            return_type_error!(
                format!(
                    "Argument for {} {} in {} has incorrect type. {} {} {}",
                    diagnostics.slot_noun(),
                    slot_label,
                    diagnostics.callable_label(),
                    expected_found_clause(
                        &expectation.data_type,
                        &argument.value.data_type,
                        string_table
                    ),
                    offending_value_clause(&argument.value, string_table),
                    conversion_hint
                ),
                argument.location.clone(),
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => diagnostics.primary_conversion_suggestion(),
                }
            );
        }

        ordered.push(argument.with_passing_mode(passing_mode));
    }

    Ok(ordered)
}

fn classify_call_passing_mode(
    diagnostics: &CallDiagnosticContext<'_>,
    argument: &CallArgument,
    expectation: &ParameterExpectation,
    _location: SourceLocation,
    string_table: &StringTable,
) -> Result<CallPassingMode, CompilerError> {
    let slot_noun = diagnostics.slot_noun();
    let parameter_label = expectation
        .name
        .map(|name| format!("{slot_noun} '{}'", string_table.resolve(name)))
        .unwrap_or_else(|| {
            argument
                .target_param
                .map(|name| format!("{slot_noun} '{}'", string_table.resolve(name)))
                .unwrap_or_else(|| format!("this {slot_noun}"))
        });
    match (argument.access_mode, &expectation.access_mode) {
        (CallAccessMode::Shared, ExpectedAccessMode::Shared) => Ok(CallPassingMode::Shared),
        (CallAccessMode::Shared, ExpectedAccessMode::Mutable) => {
            if !expression_is_place(&argument.value) {
                return Ok(CallPassingMode::FreshMutableValue);
            }
            return_rule_error!(
                format!(
                    "{} requires explicit '~' for {}",
                    diagnostics.callable_label(),
                    parameter_label
                ),
                argument.location.clone(),
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => "Add '~' to this argument",
                }
            )
        }
        (CallAccessMode::Mutable, ExpectedAccessMode::Shared) => {
            return_rule_error!(
                format!(
                    "{} does not accept '~' for {}",
                    diagnostics.callable_label(),
                    parameter_label
                ),
                argument.location.clone(),
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => "Remove '~' from this argument",
                }
            )
        }
        (CallAccessMode::Mutable, ExpectedAccessMode::Mutable) => {
            if !expression_is_place(&argument.value) {
                return_rule_error!(
                    format!(
                        "{} received '~' on a non-place argument for {}. Pass fresh values without '~'.",
                        diagnostics.callable_label(), parameter_label
                    ),
                    argument.location.clone(),
                    {
                        CompilationStage => "Function Call Validation",
                        PrimarySuggestion => "Use '~' only on a mutable place, or pass fresh values without '~'",
                    }
                );
            }
            if !expression_is_mutable_place(&argument.value) {
                return_rule_error!(
                    format!(
                        "{} received '~' on an immutable place for {}",
                        diagnostics.callable_label(),
                        parameter_label
                    ),
                    argument.location.clone(),
                    {
                        CompilationStage => "Function Call Validation",
                        PrimarySuggestion => "Use a mutable variable or remove '~'",
                    }
                );
            }
            Ok(CallPassingMode::MutablePlace)
        }
    }
}

fn is_call_argument_type_compatible(
    expectation: &ParameterExpectation,
    argument: &CallArgument,
    passing_mode: CallPassingMode,
) -> bool {
    if is_type_compatible(&expectation.data_type, &argument.value.data_type) {
        return true;
    }

    if passing_mode != CallPassingMode::FreshMutableValue {
        return false;
    }

    fresh_mutable_rvalue_type_compatible(&expectation.data_type, &argument.value.data_type)
}

fn fresh_mutable_rvalue_type_compatible(expected: &DataType, actual: &DataType) -> bool {
    match (expected, actual) {
        // Fresh collection literals are produced as immutable-owned values by default, but
        // mutable call slots own and materialize their own hidden local before the call.
        // Inner element type compatibility still has to hold.
        (DataType::Collection(expected_inner), DataType::Collection(actual_inner)) => {
            is_type_compatible(expected_inner.as_ref(), actual_inner.as_ref())
        }
        _ => false,
    }
}

fn expression_is_place(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Reference(_) => true,
        ExpressionKind::Runtime(nodes) if nodes.len() == 1 => ast_node_is_place(&nodes[0]),
        _ => false,
    }
}

fn expression_is_mutable_place(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Reference(_) => expression.value_mode.is_mutable(),
        ExpressionKind::Runtime(nodes) if nodes.len() == 1 => ast_node_is_mutable_place(&nodes[0]),
        _ => false,
    }
}
