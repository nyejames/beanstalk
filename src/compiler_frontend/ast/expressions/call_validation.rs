use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::call_argument::{CallAccessMode, CallArgument};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::field_access::{ast_node_is_mutable_place, ast_node_is_place};
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::host_functions::{HostAccessKind, HostFunctionDef};
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::type_coercion::CompatibilityContext;
use crate::compiler_frontend::type_coercion::compatibility::is_type_compatible;
use crate::compiler_frontend::type_coercion::diagnostics::argument_conversion_hint;
use crate::return_rule_error;
use crate::return_type_error;
use rustc_hash::FxHashMap;

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

pub(crate) fn expectations_from_user_parameters(
    parameters: &[Declaration],
) -> Vec<ParameterExpectation> {
    parameters
        .iter()
        .map(|parameter| ParameterExpectation {
            name: parameter.id.name(),
            data_type: parameter.value.data_type.clone(),
            access_mode: if parameter.value.ownership.is_mutable() {
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
    function: &HostFunctionDef,
) -> Vec<ParameterExpectation> {
    function
        .parameters
        .iter()
        .map(|parameter| ParameterExpectation {
            name: None,
            data_type: parameter.language_type.clone(),
            access_mode: match parameter.access_kind {
                HostAccessKind::Shared => ExpectedAccessMode::Shared,
                HostAccessKind::Mutable => ExpectedAccessMode::Mutable,
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
            // Constructor arguments are always passed by shared value; the field's own
            // declared mutability applies after construction, not at the call site.
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

pub(crate) fn resolve_call_arguments(
    call_name: &str,
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
                    String::from("This call accepts positional-only parameters.")
                } else {
                    format!("Known parameters: {}", known_parameters.join(", "))
                };
                return_rule_error!(
                    format!(
                        "Function '{}' has no parameter named '{}'. {}",
                        call_name,
                        string_table.resolve(target_name),
                        known_parameter_hint
                    ),
                    argument
                        .target_location
                        .clone()
                        .unwrap_or_else(|| argument.location.clone()),
                    {
                        CompilationStage => "Function Call Validation",
                        PrimarySuggestion => "Use a declared parameter name in this call",
                    }
                );
            };
            slot
        } else {
            if saw_named_argument {
                return_rule_error!(
                    format!(
                        "Function '{}' does not allow positional arguments after named arguments",
                        call_name
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
                        "Function '{}' expects {} argument(s), but extra positional arguments were provided",
                        call_name,
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
            let parameter_name = expectations[slot]
                .name
                .map(|name| string_table.resolve(name).to_owned())
                .unwrap_or_else(|| format!("#{}", slot + 1));
            return_rule_error!(
                format!("Parameter '{}' was provided more than once", parameter_name),
                argument
                    .target_location
                    .clone()
                    .unwrap_or_else(|| argument.location.clone()),
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => "Provide each parameter at most once",
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
                    format!("Missing required argument for parameter {}", parameter_label),
                    location.clone(),
                    {
                        CompilationStage => "Function Call Validation",
                        PrimarySuggestion => "Provide values for all required parameters",
                    }
                );
            }
        }
    }

    let mut ordered = Vec::with_capacity(expectations.len());
    for (slot, expectation) in expectations.iter().enumerate() {
        let argument = resolved[slot]
            .take()
            .expect("resolved argument slots must be filled before validation");

        if !is_type_compatible(
            &expectation.data_type,
            &argument.value.data_type,
            CompatibilityContext::Exact,
        ) {
            let conversion_hint =
                argument_conversion_hint(&expectation.data_type, &argument.value.data_type);
            return_type_error!(
                format!(
                    "Argument for parameter {} in function '{}' has incorrect type. Expected {}, but got {}. {}",
                    expectation
                        .name
                        .map(|name| format!("'{}'", string_table.resolve(name)))
                        .unwrap_or_else(|| format!("#{}", slot + 1)),
                    call_name,
                    expectation.data_type.display_with_table(string_table),
                    argument.value.data_type.display_with_table(string_table),
                    conversion_hint,
                ),
                argument.location.clone(),
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => "Convert the argument to the expected type",
                }
            );
        }

        validate_call_access_mode(
            call_name,
            &argument,
            expectation,
            location.clone(),
            string_table,
        )?;
        ordered.push(argument);
    }

    Ok(ordered)
}

fn validate_call_access_mode(
    call_name: &str,
    argument: &CallArgument,
    expectation: &ParameterExpectation,
    _location: SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    let parameter_label = expectation
        .name
        .map(|name| format!("parameter '{}'", string_table.resolve(name)))
        .unwrap_or_else(|| {
            argument
                .target_param
                .map(|name| format!("parameter '{}'", string_table.resolve(name)))
                .unwrap_or_else(|| String::from("this parameter"))
        });
    match (argument.access_mode, &expectation.access_mode) {
        (CallAccessMode::Shared, ExpectedAccessMode::Shared) => Ok(()),
        (CallAccessMode::Shared, ExpectedAccessMode::Mutable) => {
            return_rule_error!(
                format!(
                    "Function '{}' requires explicit '~' for {}",
                    call_name, parameter_label
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
                    "Function '{}' does not accept '~' for {}",
                    call_name, parameter_label
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
                        "Function '{}' received '~' on a non-place argument for {}",
                        call_name, parameter_label
                    ),
                    argument.location.clone(),
                    {
                        CompilationStage => "Function Call Validation",
                        PrimarySuggestion => "Use '~' with a mutable variable or mutable field place",
                    }
                );
            }
            if !expression_is_mutable_place(&argument.value) {
                return_rule_error!(
                    format!(
                        "Function '{}' received '~' on an immutable place for {}",
                        call_name, parameter_label
                    ),
                    argument.location.clone(),
                    {
                        CompilationStage => "Function Call Validation",
                        PrimarySuggestion => "Use a mutable variable or remove '~'",
                    }
                );
            }
            Ok(())
        }
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
        ExpressionKind::Reference(_) => expression.ownership.is_mutable(),
        ExpressionKind::Runtime(nodes) if nodes.len() == 1 => ast_node_is_mutable_place(&nodes[0]),
        _ => false,
    }
}
