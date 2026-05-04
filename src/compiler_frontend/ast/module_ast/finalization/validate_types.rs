//! Final AST type-boundary validation before HIR lowering.
//!
//! WHAT: rejects unresolved frontend-only type shapes in executable AST values.
//! WHY: AST owns name/type resolution; HIR should only receive concrete semantic types,
//! with collection generic instances as the one builtin generic exception.

use super::finalizer::AstFinalizer;
use crate::compiler_frontend::ast::ast_nodes::{
    AstNode, Declaration, LoopBindings, MultiBindTarget, NodeKind,
};
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ResultCallHandling,
};
use crate::compiler_frontend::ast::statements::functions::{FunctionReturn, FunctionSignature};
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::templates::template::TemplateAtom;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::generics::{BuiltinGenericType, GenericBaseType};
use crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantPayload;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

impl AstFinalizer<'_, '_, '_> {
    pub(crate) fn validate_no_unresolved_executable_types(
        &self,
        ast: &[AstNode],
        module_constants: &[Declaration],
        string_table: &StringTable,
    ) -> Result<(), CompilerError> {
        for node in ast {
            validate_node(node, string_table)?;
        }

        for constant in module_constants {
            validate_declaration(constant, string_table)?;
        }

        Ok(())
    }
}

// --------------------------
//  Node validation
// --------------------------

fn validate_node(node: &AstNode, string_table: &StringTable) -> Result<(), CompilerError> {
    match &node.kind {
        NodeKind::Return(values) => validate_expressions(values, string_table),

        NodeKind::ReturnError(value)
        | NodeKind::PushStartRuntimeFragment(value)
        | NodeKind::Rvalue(value) => validate_expression(value, string_table),

        NodeKind::If(condition, then_body, else_body) => {
            validate_expression(condition, string_table)?;
            validate_nodes(then_body, string_table)?;
            if let Some(else_body) = else_body {
                validate_nodes(else_body, string_table)?;
            }
            Ok(())
        }

        NodeKind::Match {
            scrutinee,
            arms,
            default,
            exhaustiveness: _,
        } => {
            validate_expression(scrutinee, string_table)?;
            for arm in arms {
                match &arm.pattern {
                    MatchPattern::Literal(value) | MatchPattern::Relational { value, .. } => {
                        validate_expression(value, string_table)?;
                    }
                    MatchPattern::ChoiceVariant { captures, .. } => {
                        for capture in captures {
                            validate_type(&capture.field_type, &capture.location, string_table)?;
                        }
                    }
                    MatchPattern::Wildcard { .. } | MatchPattern::Capture { .. } => {}
                }
                if let Some(guard) = &arm.guard {
                    validate_expression(guard, string_table)?;
                }
                validate_nodes(&arm.body, string_table)?;
            }
            if let Some(default_body) = default {
                validate_nodes(default_body, string_table)?;
            }
            Ok(())
        }

        NodeKind::ScopedBlock { body } => validate_nodes(body, string_table),

        NodeKind::RangeLoop {
            bindings,
            range,
            body,
        } => {
            validate_loop_bindings(bindings, string_table)?;
            validate_expression(&range.start, string_table)?;
            validate_expression(&range.end, string_table)?;
            if let Some(step) = &range.step {
                validate_expression(step, string_table)?;
            }
            validate_nodes(body, string_table)
        }

        NodeKind::CollectionLoop {
            bindings,
            iterable,
            body,
        } => {
            validate_loop_bindings(bindings, string_table)?;
            validate_expression(iterable, string_table)?;
            validate_nodes(body, string_table)
        }

        NodeKind::WhileLoop(condition, body) => {
            validate_expression(condition, string_table)?;
            validate_nodes(body, string_table)
        }

        NodeKind::VariableDeclaration(declaration) => {
            validate_declaration(declaration, string_table)
        }

        NodeKind::FieldAccess {
            base, data_type, ..
        } => {
            validate_node(base, string_table)?;
            validate_type(data_type, &node.location, string_table)
        }

        NodeKind::MethodCall {
            receiver,
            args,
            result_types,
            ..
        }
        | NodeKind::CollectionBuiltinCall {
            receiver,
            args,
            result_types,
            ..
        } => {
            validate_node(receiver, string_table)?;
            validate_call_arguments(args, string_table)?;
            validate_types(result_types, &node.location, string_table)
        }

        NodeKind::FunctionCall {
            args, result_types, ..
        }
        | NodeKind::ResultHandledFunctionCall {
            args, result_types, ..
        }
        | NodeKind::HostFunctionCall {
            args, result_types, ..
        } => {
            validate_call_arguments(args, string_table)?;
            validate_types(result_types, &node.location, string_table)
        }

        NodeKind::StructDefinition(_, fields) => validate_declarations(fields, string_table),

        NodeKind::Function(_, signature, body) => {
            validate_signature(signature, &node.location, string_table)?;
            validate_nodes(body, string_table)
        }

        NodeKind::Assignment { target, value } => {
            validate_node(target, string_table)?;
            validate_expression(value, string_table)
        }

        NodeKind::MultiBind { targets, value } => {
            for target in targets {
                validate_multi_bind_target(target, string_table)?;
            }
            validate_expression(value, string_table)
        }

        NodeKind::Break | NodeKind::Continue | NodeKind::Operator(_) => Ok(()),
    }
}

// --------------------------
//  Expression validation
// --------------------------

fn validate_expression(
    expression: &Expression,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    validate_type(&expression.data_type, &expression.location, string_table)?;

    match &expression.kind {
        ExpressionKind::Runtime(nodes) => validate_nodes(nodes, string_table),

        ExpressionKind::Copy(place) => validate_node(place, string_table),

        ExpressionKind::Function(signature, body) => {
            validate_signature(signature, &expression.location, string_table)?;
            validate_nodes(body, string_table)
        }

        ExpressionKind::FunctionCall(_, args) | ExpressionKind::HostFunctionCall(_, args) => {
            validate_call_arguments(args, string_table)
        }

        ExpressionKind::ResultHandledFunctionCall { args, handling, .. } => {
            validate_call_arguments(args, string_table)?;
            validate_result_handling(handling, string_table)
        }

        ExpressionKind::BuiltinCast { value, .. }
        | ExpressionKind::ResultConstruct { value, .. }
        | ExpressionKind::Coerced { value, .. } => validate_expression(value, string_table),

        ExpressionKind::HandledResult { value, handling } => {
            validate_expression(value, string_table)?;
            validate_result_handling(handling, string_table)
        }

        ExpressionKind::Template(template) => {
            for atom in &template.content.atoms {
                let TemplateAtom::Content(segment) = atom else {
                    continue;
                };
                validate_expression(&segment.expression, string_table)?;
            }
            Ok(())
        }

        ExpressionKind::Collection(items) => validate_expressions(items, string_table),

        ExpressionKind::StructDefinition(fields)
        | ExpressionKind::StructInstance(fields)
        | ExpressionKind::ChoiceConstruct { fields, .. } => {
            validate_declarations(fields, string_table)
        }

        ExpressionKind::Range(start, end) => {
            validate_expression(start, string_table)?;
            validate_expression(end, string_table)
        }

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

fn validate_result_handling(
    handling: &ResultCallHandling,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    match handling {
        ResultCallHandling::Propagate => Ok(()),
        ResultCallHandling::Fallback(values) => validate_expressions(values, string_table),
        ResultCallHandling::Handler { fallback, body, .. } => {
            if let Some(values) = fallback {
                validate_expressions(values, string_table)?;
            }
            validate_nodes(body, string_table)
        }
    }
}

fn validate_loop_bindings(
    bindings: &LoopBindings,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if let Some(item) = &bindings.item {
        validate_declaration(item, string_table)?;
    }
    if let Some(index) = &bindings.index {
        validate_declaration(index, string_table)?;
    }
    Ok(())
}

fn validate_call_arguments(
    arguments: &[CallArgument],
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    for argument in arguments {
        validate_expression(&argument.value, string_table)?;
    }
    Ok(())
}

fn validate_signature(
    signature: &FunctionSignature,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    validate_declarations(&signature.parameters, string_table)?;
    for return_slot in &signature.returns {
        match &return_slot.value {
            FunctionReturn::Value(data_type)
            | FunctionReturn::AliasCandidates { data_type, .. } => {
                validate_type(data_type, location, string_table)?;
            }
        }
    }
    Ok(())
}

fn validate_declarations(
    declarations: &[Declaration],
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    for declaration in declarations {
        validate_declaration(declaration, string_table)?;
    }
    Ok(())
}

fn validate_declaration(
    declaration: &Declaration,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    validate_expression(&declaration.value, string_table)
}

fn validate_multi_bind_target(
    target: &MultiBindTarget,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    validate_type(&target.data_type, &target.location, string_table)
}

fn validate_nodes(nodes: &[AstNode], string_table: &StringTable) -> Result<(), CompilerError> {
    for node in nodes {
        validate_node(node, string_table)?;
    }
    Ok(())
}

fn validate_expressions(
    expressions: &[Expression],
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    for expression in expressions {
        validate_expression(expression, string_table)?;
    }
    Ok(())
}

fn validate_types(
    data_types: &[DataType],
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    for data_type in data_types {
        validate_type(data_type, location, string_table)?;
    }
    Ok(())
}

fn validate_type(
    data_type: &DataType,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    match data_type {
        DataType::NamedType(_) | DataType::TypeParameter { .. } => {
            unresolved_type_error(data_type, location, string_table)
        }

        DataType::GenericInstance {
            base: GenericBaseType::Builtin(BuiltinGenericType::Collection),
            arguments,
        } => validate_types(arguments, location, string_table),

        DataType::GenericInstance { .. } => {
            unresolved_type_error(data_type, location, string_table)
        }

        DataType::Option(inner) | DataType::Reference(inner) => {
            validate_type(inner, location, string_table)
        }

        DataType::Result { ok, err } => {
            validate_type(ok, location, string_table)?;
            validate_type(err, location, string_table)
        }

        DataType::Returns(values) => validate_types(values, location, string_table),

        DataType::Function(_, signature) => validate_signature(signature, location, string_table),

        DataType::Struct { fields, .. } | DataType::Parameters(fields) => {
            validate_declarations(fields, string_table)
        }

        DataType::Choices { variants, .. } => {
            for variant in variants {
                let ChoiceVariantPayload::Record { fields } = &variant.payload else {
                    continue;
                };
                validate_declarations(fields, string_table)?;
            }
            Ok(())
        }

        _ => Ok(()),
    }
}

fn unresolved_type_error(
    data_type: &DataType,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    Err(CompilerError::new_type_error(
        format!(
            "Unresolved generic or named type '{}' reached executable AST after type resolution.",
            data_type.display_with_table(string_table)
        ),
        location.to_owned(),
    ))
}
