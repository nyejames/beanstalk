//! Final AST type-boundary validation before HIR lowering.
//!
//! WHAT: validates that HIR-bound AST values carry TypeIds registered in the module
//! `TypeEnvironment`.
//! WHY: AST owns name/type resolution; HIR should receive canonical semantic type identity,
//! not diagnostic-only `DataType` reconstructions.

use super::finalizer::AstFinalizer;
use crate::compiler_frontend::ast::ast_nodes::{
    AstNode, Declaration, LoopBindings, MultiBindTarget, NodeKind,
};
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, FallibleHandling,
};
use crate::compiler_frontend::ast::expressions::expression_types::CastHandling;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::statements::value_production::types::ValueBlock;
use crate::compiler_frontend::ast::templates::template::TemplateAtom;
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorType};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

impl AstFinalizer<'_, '_> {
    pub(crate) fn validate_no_unresolved_executable_types(
        &self,
        ast: &[AstNode],
        module_constants: &[Declaration],
        string_table: &StringTable,
    ) -> Result<(), CompilerError> {
        let type_environment = &self.environment.type_environment;

        for node in ast {
            validate_node(node, type_environment, string_table)?;
        }

        for constant in module_constants {
            validate_declaration(constant, type_environment, string_table)?;
        }

        Ok(())
    }
}

// --------------------------
//  Node validation
// --------------------------

/// Recursively validates all type-carrying positions inside an AST node.
///
/// WHAT: Walks every recursive sub-position in the node (expressions, nested
/// statement bodies, pattern captures, call arguments) and asserts that each
/// `TypeId` exists in the module `TypeEnvironment`.
///
/// WHY: AST owns semantic type resolution; HIR must receive only canonical
/// `TypeId`s. Any missing entry indicates a compiler bug.
fn validate_node(
    node: &AstNode,
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    match &node.kind {
        // Control flow with nested statement bodies.
        NodeKind::If(condition, then_body, else_body) => {
            validate_expression(condition, type_environment, string_table)?;
            validate_nodes(then_body, type_environment, string_table)?;
            if let Some(else_body) = else_body {
                validate_nodes(else_body, type_environment, string_table)?;
            }
            Ok(())
        }

        NodeKind::Match {
            scrutinee,
            arms,
            default,
            exhaustiveness: _,
        } => {
            validate_expression(scrutinee, type_environment, string_table)?;

            for arm in arms {
                match &arm.pattern {
                    MatchPattern::Literal(value)
                    | MatchPattern::OptionValue { value, .. }
                    | MatchPattern::Relational { value, .. } => {
                        validate_expression(value, type_environment, string_table)?;
                    }
                    MatchPattern::ChoiceVariant { captures, .. } => {
                        for capture in captures {
                            validate_type_id(capture.type_id, &capture.location, type_environment)?;
                        }
                    }
                    MatchPattern::OptionNone { .. }
                    | MatchPattern::Wildcard { .. }
                    | MatchPattern::Capture { .. }
                    | MatchPattern::OptionPresentCapture { .. } => {}
                }

                if let Some(guard) = &arm.guard {
                    validate_expression(guard, type_environment, string_table)?;
                }

                validate_nodes(&arm.body, type_environment, string_table)?;
            }

            if let Some(default_body) = default {
                validate_nodes(default_body, type_environment, string_table)?;
            }

            Ok(())
        }

        NodeKind::ScopedBlock { body } => validate_nodes(body, type_environment, string_table),

        NodeKind::RangeLoop {
            bindings,
            range,
            body,
        } => {
            validate_loop_bindings(bindings, type_environment, string_table)?;
            validate_expression(&range.start, type_environment, string_table)?;
            validate_expression(&range.end, type_environment, string_table)?;
            if let Some(step) = &range.step {
                validate_expression(step, type_environment, string_table)?;
            }
            validate_nodes(body, type_environment, string_table)
        }

        NodeKind::CollectionLoop {
            bindings,
            iterable,
            body,
        } => {
            validate_loop_bindings(bindings, type_environment, string_table)?;
            validate_expression(iterable, type_environment, string_table)?;
            validate_nodes(body, type_environment, string_table)
        }

        NodeKind::WhileLoop(condition, body) => {
            validate_expression(condition, type_environment, string_table)?;
            validate_nodes(body, type_environment, string_table)
        }

        // Terminal expressions that carry a single value.
        NodeKind::Return(values) => validate_expressions(values, type_environment, string_table),

        NodeKind::ReturnError(value)
        | NodeKind::PushStartRuntimeFragment(value)
        | NodeKind::Rvalue(value) => validate_expression(value, type_environment, string_table),

        // Declarations and assignments.
        NodeKind::VariableDeclaration(declaration) => {
            validate_declaration(declaration, type_environment, string_table)
        }

        NodeKind::Assignment { target, value } => {
            validate_node(target, type_environment, string_table)?;
            validate_expression(value, type_environment, string_table)
        }

        NodeKind::MultiBind { targets, value } => {
            for target in targets {
                validate_multi_bind_target(target, type_environment)?;
            }
            validate_expression(value, type_environment, string_table)
        }

        // Field access and calls.
        NodeKind::FieldAccess { base, type_id, .. } => {
            validate_node(base, type_environment, string_table)?;
            validate_type_id(*type_id, &node.location, type_environment)
        }

        NodeKind::MethodCall {
            receiver,
            args,
            result_type_ids,
            ..
        }
        | NodeKind::CollectionBuiltinCall {
            receiver,
            args,
            result_type_ids,
            ..
        }
        | NodeKind::MapBuiltinCall {
            receiver,
            args,
            result_type_ids,
            ..
        } => {
            validate_node(receiver, type_environment, string_table)?;
            validate_call_arguments(args, type_environment, string_table)?;
            validate_type_ids(result_type_ids, &node.location, type_environment)
        }

        NodeKind::FunctionCall {
            args,
            result_type_ids,
            ..
        }
        | NodeKind::HandledFallibleFunctionCall {
            args,
            result_type_ids,
            ..
        }
        | NodeKind::HostFunctionCall {
            args,
            result_type_ids,
            ..
        } => {
            validate_call_arguments(args, type_environment, string_table)?;
            validate_type_ids(result_type_ids, &node.location, type_environment)
        }

        NodeKind::HandledFallibleHostFunctionCall {
            args,
            result_type_ids,
            error_type_id,
            ..
        } => {
            validate_call_arguments(args, type_environment, string_table)?;
            validate_type_ids(result_type_ids, &node.location, type_environment)?;
            validate_type_ids(&[*error_type_id], &node.location, type_environment)
        }

        // Type and function definitions.
        NodeKind::StructDefinition(_, fields) => {
            validate_declarations(fields, type_environment, string_table)
        }

        NodeKind::Function(_, signature, body) => {
            validate_signature(signature, &node.location, type_environment, string_table)?;
            validate_nodes(body, type_environment, string_table)
        }

        NodeKind::Assert { condition, .. } => {
            validate_expression(condition, type_environment, string_table)
        }

        // Terminal nodes that contain no type-carrying positions.
        NodeKind::Break | NodeKind::Continue | NodeKind::Operator(_) => Ok(()),

        // Value-producing terminator inside an active value block.
        NodeKind::ThenValue(produced_values) => {
            validate_expressions(&produced_values.expressions, type_environment, string_table)
        }
    }
}

// --------------------------
//  Expression validation
// --------------------------

/// Recursively validates all type-carrying positions inside an expression.
///
/// WHAT: Validates the expression's own `type_id`, then recursively checks
/// nested expressions, call arguments, templates, and sub-nodes.
///
/// WHY: Expressions are the leaves and branches of the AST value tree;
/// unresolved types here would propagate into HIR as invalid semantic identity.
fn validate_expression(
    expression: &Expression,
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    validate_type_id(expression.type_id, &expression.location, type_environment)?;

    match &expression.kind {
        // Recursive expression containers.
        ExpressionKind::Runtime(nodes) => validate_nodes(nodes, type_environment, string_table),

        ExpressionKind::Copy(place) => validate_node(place, type_environment, string_table),

        // Function expressions.
        ExpressionKind::Function(signature, body) => {
            validate_signature(
                signature,
                &expression.location,
                type_environment,
                string_table,
            )?;
            validate_nodes(body, type_environment, string_table)
        }

        // Calls.
        ExpressionKind::FunctionCall { args, .. }
        | ExpressionKind::HostFunctionCall { args, .. } => {
            validate_call_arguments(args, type_environment, string_table)
        }

        ExpressionKind::HandledFallibleFunctionCall { args, handling, .. } => {
            validate_call_arguments(args, type_environment, string_table)?;
            validate_fallible_handling(handling, type_environment, string_table)
        }

        ExpressionKind::HandledFallibleHostFunctionCall {
            args,
            error_type_id,
            handling,
            ..
        } => {
            validate_call_arguments(args, type_environment, string_table)?;
            validate_type_ids(&[*error_type_id], &expression.location, type_environment)?;
            validate_fallible_handling(handling, type_environment, string_table)
        }

        // Wrapped and coerced values.
        ExpressionKind::FallibleCarrierConstruct { value, .. }
        | ExpressionKind::OptionPropagation { value }
        | ExpressionKind::Coerced { value, .. } => {
            validate_expression(value, type_environment, string_table)
        }

        ExpressionKind::Cast(cast) => {
            validate_expression(&cast.source, type_environment, string_table)?;
            validate_type_id(cast.target_type_id, &cast.location, type_environment)?;
            validate_type_id(cast.source_type_id, &cast.source.location, type_environment)?;
            validate_cast_handling(&cast.handling, type_environment, string_table)
        }

        ExpressionKind::HandledFallibleExpression { value, handling } => {
            validate_expression(value, type_environment, string_table)?;
            validate_fallible_handling(handling, type_environment, string_table)
        }

        // Template and collection literals.
        ExpressionKind::Template(template) => {
            for atom in &template.content.atoms {
                let TemplateAtom::Content(segment) = atom else {
                    continue;
                };
                validate_expression(&segment.expression, type_environment, string_table)?;
            }
            Ok(())
        }

        ExpressionKind::Collection(items) => {
            validate_expressions(items, type_environment, string_table)
        }

        // Struct and choice constructors.
        ExpressionKind::StructDefinition(fields)
        | ExpressionKind::StructInstance(fields)
        | ExpressionKind::ChoiceConstruct { fields, .. } => {
            validate_declarations(fields, type_environment, string_table)
        }

        // Range expressions.
        ExpressionKind::Range(start, end) => {
            validate_expression(start, type_environment, string_table)?;
            validate_expression(end, type_environment, string_table)
        }

        ExpressionKind::ValueBlock { block } => match block.as_ref() {
            ValueBlock::If(value_if) => {
                validate_expression(&value_if.condition, type_environment, string_table)?;
                validate_nodes(&value_if.then_body, type_environment, string_table)?;
                validate_nodes(&value_if.else_body, type_environment, string_table)
            }
            ValueBlock::Match(value_match) => {
                validate_expression(&value_match.scrutinee, type_environment, string_table)?;
                for arm in &value_match.arms {
                    if let Some(guard) = &arm.guard {
                        validate_expression(guard, type_environment, string_table)?;
                    }
                    validate_nodes(&arm.body, type_environment, string_table)?;
                }
                if let Some(default_body) = &value_match.default {
                    validate_nodes(default_body, type_environment, string_table)?;
                }
                Ok(())
            }
            ValueBlock::Catch(value_catch) => {
                validate_expression(&value_catch.handled_value, type_environment, string_table)
            }
        },

        ExpressionKind::MapLiteral(entries) => {
            for entry in entries {
                validate_expression(&entry.key, type_environment, string_table)?;
                validate_expression(&entry.value, type_environment, string_table)?;
            }
            Ok(())
        }

        // Terminal literals and references — types were resolved at construction.
        ExpressionKind::NoValue
        | ExpressionKind::OptionNone
        | ExpressionKind::Int(_)
        | ExpressionKind::Float(_)
        | ExpressionKind::StringSlice(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Path(_)
        | ExpressionKind::Reference(_) => Ok(()),
    }
}

// --------------------------
//  Helpers
// --------------------------

fn validate_fallible_handling(
    handling: &FallibleHandling,
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    match handling {
        FallibleHandling::Propagate => Ok(()),
        FallibleHandling::Handler { body, .. } => {
            validate_nodes(body, type_environment, string_table)
        }
    }
}

fn validate_cast_handling(
    handling: &CastHandling,
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    match handling {
        CastHandling::Infallible | CastHandling::Propagate => Ok(()),
        CastHandling::Recover(handling) => {
            validate_fallible_handling(handling, type_environment, string_table)
        }
    }
}

fn validate_loop_bindings(
    bindings: &LoopBindings,
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if let Some(item) = &bindings.item {
        validate_declaration(item, type_environment, string_table)?;
    }

    if let Some(index) = &bindings.index {
        validate_declaration(index, type_environment, string_table)?;
    }

    Ok(())
}

fn validate_call_arguments(
    arguments: &[CallArgument],
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    for argument in arguments {
        validate_expression(&argument.value, type_environment, string_table)?;
    }
    Ok(())
}

fn validate_signature(
    signature: &FunctionSignature,
    location: &SourceLocation,
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    validate_declarations(&signature.parameters, type_environment, string_table)?;
    validate_type_ids(
        &signature.success_return_type_ids(),
        location,
        type_environment,
    )?;
    if let Some(error_return_type_id) = signature.error_return_type_id() {
        validate_type_id(error_return_type_id, location, type_environment)?;
    }
    Ok(())
}

fn validate_declarations(
    declarations: &[Declaration],
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    for declaration in declarations {
        validate_declaration(declaration, type_environment, string_table)?;
    }
    Ok(())
}

fn validate_declaration(
    declaration: &Declaration,
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    validate_expression(&declaration.value, type_environment, string_table)
}

fn validate_multi_bind_target(
    target: &MultiBindTarget,
    type_environment: &TypeEnvironment,
) -> Result<(), CompilerError> {
    validate_type_id(target.type_id, &target.location, type_environment)
}

fn validate_nodes(
    nodes: &[AstNode],
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    for node in nodes {
        validate_node(node, type_environment, string_table)?;
    }
    Ok(())
}

fn validate_expressions(
    expressions: &[Expression],
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    for expression in expressions {
        validate_expression(expression, type_environment, string_table)?;
    }
    Ok(())
}

fn validate_type_ids(
    type_ids: &[TypeId],
    location: &SourceLocation,
    type_environment: &TypeEnvironment,
) -> Result<(), CompilerError> {
    for type_id in type_ids {
        validate_type_id(*type_id, location, type_environment)?;
    }
    Ok(())
}

/// Asserts that a single `TypeId` is registered in the `TypeEnvironment`.
///
/// WHY: A missing `TypeId` at this stage means AST type resolution failed to
/// record a canonical type for a value position. This is an internal compiler
/// invariant, not a user-facing diagnostic.
fn validate_type_id(
    type_id: TypeId,
    location: &SourceLocation,
    type_environment: &TypeEnvironment,
) -> Result<(), CompilerError> {
    if type_environment.get(type_id).is_some() {
        return Ok(());
    }

    Err(CompilerError::new(
        format!(
            "Resolved TypeId({}) reached executable AST without a matching TypeEnvironment entry.",
            type_id.0
        ),
        location.to_owned(),
        ErrorType::Compiler,
    ))
}
