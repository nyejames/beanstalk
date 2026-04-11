//! AST-local symbol/reference and mutation analysis helpers.
//!
//! WHAT: provides shared traversals for collecting referenced symbols and checking
//! tracked-symbol mutation through AST statements and expressions.
//! WHY: multiple frontend passes need identical symbol/mutation walks; keeping one
//! implementation avoids drift and duplicated traversal logic.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::{CallAccessMode, CallArgument};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ResultCallHandling,
};
use crate::compiler_frontend::datatypes::Ownership;
use crate::compiler_frontend::interned_path::InternedPath;
use rustc_hash::FxHashSet;

/// WHAT: checks whether any branch of a result-call handler may mutate tracked symbols.
/// WHY: result handlers appear across node/expression variants and should share one policy.
fn handling_may_mutate_tracked_symbols(
    handling: &ResultCallHandling,
    tracked_symbols: &FxHashSet<InternedPath>,
) -> bool {
    match handling {
        ResultCallHandling::Fallback(fallback_values) => fallback_values
            .iter()
            .any(|fallback| expression_may_mutate_tracked_symbols(fallback, tracked_symbols)),
        ResultCallHandling::Handler { fallback, body, .. } => {
            fallback.as_ref().is_some_and(|fallback_values| {
                fallback_values.iter().any(|fallback| {
                    expression_may_mutate_tracked_symbols(fallback, tracked_symbols)
                })
            }) || body
                .iter()
                .any(|node| ast_node_may_mutate_tracked_symbols(node, tracked_symbols))
        }
        ResultCallHandling::Propagate => false,
    }
}

/// WHAT: walks all branches of a result-call handler and adds any referenced symbol names.
/// WHY: keeps reference-collection behavior consistent across expression/node variants.
fn collect_references_from_result_handling(
    handling: &ResultCallHandling,
    references: &mut FxHashSet<InternedPath>,
) {
    match handling {
        ResultCallHandling::Fallback(fallback_values) => {
            for fallback in fallback_values {
                collect_references_from_expression(fallback, references);
            }
        }
        ResultCallHandling::Handler { fallback, body, .. } => {
            if let Some(fallback_values) = fallback {
                for fallback in fallback_values {
                    collect_references_from_expression(fallback, references);
                }
            }
            for node in body {
                collect_references_from_ast_node(node, references);
            }
        }
        ResultCallHandling::Propagate => {}
    }
}

fn call_arguments_mutate_tracked_symbols(
    args: &[Expression],
    tracked_symbols: &FxHashSet<InternedPath>,
) -> bool {
    args.iter().any(|argument| {
        argument.ownership == Ownership::MutableOwned
            && expression_references_tracked_symbols(argument, tracked_symbols)
    })
}

fn call_named_arguments_mutate_tracked_symbols(
    args: &[CallArgument],
    tracked_symbols: &FxHashSet<InternedPath>,
) -> bool {
    args.iter().any(|argument| {
        argument.access_mode == CallAccessMode::Mutable
            && expression_references_tracked_symbols(&argument.value, tracked_symbols)
    })
}

/// WHAT: checks whether an expression may mutate any currently tracked symbol.
/// WHY: capture and replay planning needs a conservative mutation fence.
pub(crate) fn expression_may_mutate_tracked_symbols(
    expression: &Expression,
    tracked_symbols: &FxHashSet<InternedPath>,
) -> bool {
    match &expression.kind {
        ExpressionKind::FunctionCall(_, args)
        | ExpressionKind::HostFunctionCall(_, args)
        | ExpressionKind::Collection(args) => {
            call_arguments_mutate_tracked_symbols(args, tracked_symbols)
                || args.iter().any(|argument| {
                    expression_may_mutate_tracked_symbols(argument, tracked_symbols)
                })
        }

        ExpressionKind::ResultHandledFunctionCall { args, handling, .. } => {
            if call_arguments_mutate_tracked_symbols(args, tracked_symbols) {
                return true;
            }

            if args
                .iter()
                .any(|argument| expression_may_mutate_tracked_symbols(argument, tracked_symbols))
            {
                return true;
            }

            handling_may_mutate_tracked_symbols(handling, tracked_symbols)
        }

        ExpressionKind::Runtime(nodes) => nodes
            .iter()
            .any(|node| ast_node_may_mutate_tracked_symbols(node, tracked_symbols)),

        ExpressionKind::Template(template) => template
            .content
            .flatten_expressions()
            .into_iter()
            .any(|value| expression_may_mutate_tracked_symbols(&value, tracked_symbols)),

        ExpressionKind::StructDefinition(arguments) | ExpressionKind::StructInstance(arguments) => {
            arguments.iter().any(|argument| {
                expression_may_mutate_tracked_symbols(&argument.value, tracked_symbols)
            })
        }

        ExpressionKind::Range(lower, upper) => {
            expression_may_mutate_tracked_symbols(lower, tracked_symbols)
                || expression_may_mutate_tracked_symbols(upper, tracked_symbols)
        }

        ExpressionKind::Function(_, body) => body
            .iter()
            .any(|node| ast_node_may_mutate_tracked_symbols(node, tracked_symbols)),

        ExpressionKind::BuiltinCast { value, .. }
        | ExpressionKind::ResultConstruct { value, .. }
        | ExpressionKind::Coerced { value, .. } => {
            expression_may_mutate_tracked_symbols(value, tracked_symbols)
        }

        ExpressionKind::HandledResult { value, handling } => {
            expression_may_mutate_tracked_symbols(value, tracked_symbols)
                || handling_may_mutate_tracked_symbols(handling, tracked_symbols)
        }

        ExpressionKind::Copy(place) => ast_node_may_mutate_tracked_symbols(place, tracked_symbols),

        ExpressionKind::Reference(_)
        | ExpressionKind::NoValue
        | ExpressionKind::OptionNone
        | ExpressionKind::Int(_)
        | ExpressionKind::Float(_)
        | ExpressionKind::StringSlice(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Path(_) => false,
    }
}

/// WHAT: checks whether a statement may mutate any currently tracked symbol.
/// WHY: callers that replay setup statements need statement-level mutation checks.
pub(crate) fn ast_node_may_mutate_tracked_symbols(
    node: &AstNode,
    tracked_symbols: &FxHashSet<InternedPath>,
) -> bool {
    match &node.kind {
        NodeKind::VariableDeclaration(declaration) => {
            expression_may_mutate_tracked_symbols(&declaration.value, tracked_symbols)
        }

        NodeKind::Assignment { target, value } => {
            ast_node_references_tracked_symbols(target, tracked_symbols)
                || expression_may_mutate_tracked_symbols(value, tracked_symbols)
        }

        NodeKind::MethodCall { receiver, args, .. } => {
            ast_node_references_tracked_symbols(receiver, tracked_symbols)
                || call_named_arguments_mutate_tracked_symbols(args, tracked_symbols)
                || args.iter().any(|argument| {
                    expression_may_mutate_tracked_symbols(&argument.value, tracked_symbols)
                })
        }

        NodeKind::FunctionCall { args, .. } | NodeKind::HostFunctionCall { args, .. } => {
            call_named_arguments_mutate_tracked_symbols(args, tracked_symbols)
                || args.iter().any(|argument| {
                    expression_may_mutate_tracked_symbols(&argument.value, tracked_symbols)
                })
        }

        NodeKind::ResultHandledFunctionCall { args, handling, .. } => {
            if call_named_arguments_mutate_tracked_symbols(args, tracked_symbols) {
                return true;
            }

            if args.iter().any(|argument| {
                expression_may_mutate_tracked_symbols(&argument.value, tracked_symbols)
            }) {
                return true;
            }

            handling_may_mutate_tracked_symbols(handling, tracked_symbols)
        }

        NodeKind::Rvalue(expression) => {
            expression_may_mutate_tracked_symbols(expression, tracked_symbols)
        }

        NodeKind::Return(values) => values
            .iter()
            .any(|value| expression_may_mutate_tracked_symbols(value, tracked_symbols)),

        NodeKind::ReturnError(value) => {
            expression_may_mutate_tracked_symbols(value, tracked_symbols)
        }

        NodeKind::If(condition, then_body, else_body) => {
            expression_may_mutate_tracked_symbols(condition, tracked_symbols)
                || then_body.iter().any(|statement| {
                    ast_node_may_mutate_tracked_symbols(statement, tracked_symbols)
                })
                || else_body.as_ref().is_some_and(|body| {
                    body.iter().any(|statement| {
                        ast_node_may_mutate_tracked_symbols(statement, tracked_symbols)
                    })
                })
        }

        NodeKind::Match(scrutinee, arms, default) => {
            if expression_may_mutate_tracked_symbols(scrutinee, tracked_symbols) {
                return true;
            }

            if arms.iter().any(|arm| {
                expression_may_mutate_tracked_symbols(&arm.condition, tracked_symbols)
                    || arm.body.iter().any(|statement| {
                        ast_node_may_mutate_tracked_symbols(statement, tracked_symbols)
                    })
            }) {
                return true;
            }

            default.as_ref().is_some_and(|body| {
                body.iter().any(|statement| {
                    ast_node_may_mutate_tracked_symbols(statement, tracked_symbols)
                })
            })
        }

        NodeKind::RangeLoop {
            bindings,
            range,
            body,
        } => {
            bindings.item.as_ref().is_some_and(|binding| {
                expression_may_mutate_tracked_symbols(&binding.value, tracked_symbols)
            }) || bindings.index.as_ref().is_some_and(|binding| {
                expression_may_mutate_tracked_symbols(&binding.value, tracked_symbols)
            }) || expression_may_mutate_tracked_symbols(&range.start, tracked_symbols)
                || expression_may_mutate_tracked_symbols(&range.end, tracked_symbols)
                || range.step.as_ref().is_some_and(|step| {
                    expression_may_mutate_tracked_symbols(step, tracked_symbols)
                })
                || body.iter().any(|statement| {
                    ast_node_may_mutate_tracked_symbols(statement, tracked_symbols)
                })
        }

        NodeKind::CollectionLoop {
            bindings,
            iterable,
            body,
        } => {
            bindings.item.as_ref().is_some_and(|binding| {
                expression_may_mutate_tracked_symbols(&binding.value, tracked_symbols)
            }) || bindings.index.as_ref().is_some_and(|binding| {
                expression_may_mutate_tracked_symbols(&binding.value, tracked_symbols)
            }) || expression_may_mutate_tracked_symbols(iterable, tracked_symbols)
                || body.iter().any(|statement| {
                    ast_node_may_mutate_tracked_symbols(statement, tracked_symbols)
                })
        }

        NodeKind::WhileLoop(condition, body) => {
            expression_may_mutate_tracked_symbols(condition, tracked_symbols)
                || body.iter().any(|statement| {
                    ast_node_may_mutate_tracked_symbols(statement, tracked_symbols)
                })
        }

        NodeKind::FieldAccess { base, .. } => {
            ast_node_may_mutate_tracked_symbols(base, tracked_symbols)
        }

        NodeKind::MultiBind { value, .. } => {
            expression_may_mutate_tracked_symbols(value, tracked_symbols)
        }

        NodeKind::StructDefinition(_, fields) => fields
            .iter()
            .any(|field| expression_may_mutate_tracked_symbols(&field.value, tracked_symbols)),

        NodeKind::Function(_, _, body) => body
            .iter()
            .any(|statement| ast_node_may_mutate_tracked_symbols(statement, tracked_symbols)),

        NodeKind::Break | NodeKind::Continue | NodeKind::Operator(_) => false,
    }
}

/// WHAT: checks whether an AST node references any symbol in `tracked_symbols`.
/// WHY: mutation analysis needs a shared "does this place/expression touch X?" predicate.
pub(crate) fn ast_node_references_tracked_symbols(
    node: &AstNode,
    tracked_symbols: &FxHashSet<InternedPath>,
) -> bool {
    let mut references = FxHashSet::default();
    collect_references_from_ast_node(node, &mut references);
    references
        .into_iter()
        .any(|symbol| tracked_symbols.contains(&symbol))
}

/// WHAT: checks whether an expression references any symbol in `tracked_symbols`.
/// WHY: used by call-argument mutation checks and dependency filters.
pub(crate) fn expression_references_tracked_symbols(
    expression: &Expression,
    tracked_symbols: &FxHashSet<InternedPath>,
) -> bool {
    let mut references = FxHashSet::default();
    collect_references_from_expression(expression, &mut references);
    references
        .into_iter()
        .any(|symbol| tracked_symbols.contains(&symbol))
}

/// WHAT: collects symbol references reachable from an expression tree.
/// WHY: callers use this to build declaration dependency closures and prune sets.
pub(crate) fn collect_references_from_expression(
    expression: &Expression,
    references: &mut FxHashSet<InternedPath>,
) {
    match &expression.kind {
        ExpressionKind::Reference(name) => {
            references.insert(name.to_owned());
        }

        ExpressionKind::Copy(place) => {
            collect_references_from_ast_node(place, references);
        }

        ExpressionKind::Runtime(nodes) => {
            for node in nodes {
                collect_references_from_ast_node(node, references);
            }
        }

        ExpressionKind::FunctionCall(_, args)
        | ExpressionKind::HostFunctionCall(_, args)
        | ExpressionKind::Collection(args) => {
            for argument in args {
                collect_references_from_expression(argument, references);
            }
        }

        ExpressionKind::ResultHandledFunctionCall { args, handling, .. } => {
            for argument in args {
                collect_references_from_expression(argument, references);
            }
            collect_references_from_result_handling(handling, references);
        }

        ExpressionKind::BuiltinCast { value, .. } => {
            collect_references_from_expression(value, references);
        }

        ExpressionKind::ResultConstruct { value, .. } => {
            collect_references_from_expression(value, references);
        }

        ExpressionKind::HandledResult { value, handling } => {
            collect_references_from_expression(value, references);
            collect_references_from_result_handling(handling, references);
        }

        ExpressionKind::Template(template) => {
            for value in template.content.flatten_expressions() {
                collect_references_from_expression(&value, references);
            }
        }

        ExpressionKind::StructDefinition(arguments) | ExpressionKind::StructInstance(arguments) => {
            for argument in arguments {
                collect_references_from_expression(&argument.value, references);
            }
        }

        ExpressionKind::Range(lower, upper) => {
            collect_references_from_expression(lower, references);
            collect_references_from_expression(upper, references);
        }

        ExpressionKind::Function(_, body) => {
            for node in body {
                collect_references_from_ast_node(node, references);
            }
        }

        ExpressionKind::Coerced { value, .. } => {
            collect_references_from_expression(value, references);
        }

        ExpressionKind::NoValue
        | ExpressionKind::OptionNone
        | ExpressionKind::Int(_)
        | ExpressionKind::Float(_)
        | ExpressionKind::StringSlice(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Path(_) => {}
    }
}

/// WHAT: collects symbol references reachable from a statement tree.
/// WHY: statement-level dependency and pruning passes need one canonical traversal.
pub(crate) fn collect_references_from_ast_node(
    node: &AstNode,
    references: &mut FxHashSet<InternedPath>,
) {
    match &node.kind {
        NodeKind::VariableDeclaration(declaration) => {
            collect_references_from_expression(&declaration.value, references);
        }

        NodeKind::Assignment { target, value } => {
            collect_references_from_ast_node(target, references);
            collect_references_from_expression(value, references);
        }

        NodeKind::FieldAccess { base, .. } => {
            collect_references_from_ast_node(base, references);
        }

        NodeKind::MethodCall { receiver, args, .. } => {
            collect_references_from_ast_node(receiver, references);
            for argument in args {
                collect_references_from_expression(&argument.value, references);
            }
        }

        NodeKind::FunctionCall { args, .. } | NodeKind::HostFunctionCall { args, .. } => {
            for argument in args {
                collect_references_from_expression(&argument.value, references);
            }
        }

        NodeKind::ResultHandledFunctionCall { args, handling, .. } => {
            for argument in args {
                collect_references_from_expression(&argument.value, references);
            }
            collect_references_from_result_handling(handling, references);
        }

        NodeKind::MultiBind { targets: _, value } => {
            collect_references_from_expression(value, references);
        }

        NodeKind::StructDefinition(_, fields) => {
            for field in fields {
                collect_references_from_expression(&field.value, references);
            }
        }

        NodeKind::Function(_, _, body) => {
            for statement in body {
                collect_references_from_ast_node(statement, references);
            }
        }

        NodeKind::Rvalue(expression) => {
            collect_references_from_expression(expression, references);
        }

        NodeKind::Return(values) => {
            for value in values {
                collect_references_from_expression(value, references);
            }
        }

        NodeKind::ReturnError(value) => {
            collect_references_from_expression(value, references);
        }

        NodeKind::If(condition, then_body, else_body) => {
            collect_references_from_expression(condition, references);
            for statement in then_body {
                collect_references_from_ast_node(statement, references);
            }
            if let Some(else_body) = else_body {
                for statement in else_body {
                    collect_references_from_ast_node(statement, references);
                }
            }
        }

        NodeKind::Match(scrutinee, arms, default) => {
            collect_references_from_expression(scrutinee, references);
            for arm in arms {
                collect_references_from_expression(&arm.condition, references);
                for statement in &arm.body {
                    collect_references_from_ast_node(statement, references);
                }
            }
            if let Some(default_body) = default {
                for statement in default_body {
                    collect_references_from_ast_node(statement, references);
                }
            }
        }

        NodeKind::RangeLoop {
            bindings,
            range,
            body,
        } => {
            if let Some(item_binding) = &bindings.item {
                collect_references_from_expression(&item_binding.value, references);
            }
            if let Some(index_binding) = &bindings.index {
                collect_references_from_expression(&index_binding.value, references);
            }
            collect_references_from_expression(&range.start, references);
            collect_references_from_expression(&range.end, references);
            if let Some(step) = &range.step {
                collect_references_from_expression(step, references);
            }
            for statement in body {
                collect_references_from_ast_node(statement, references);
            }
        }

        NodeKind::CollectionLoop {
            bindings,
            iterable,
            body,
        } => {
            if let Some(item_binding) = &bindings.item {
                collect_references_from_expression(&item_binding.value, references);
            }
            if let Some(index_binding) = &bindings.index {
                collect_references_from_expression(&index_binding.value, references);
            }
            collect_references_from_expression(iterable, references);
            for statement in body {
                collect_references_from_ast_node(statement, references);
            }
        }

        NodeKind::WhileLoop(condition, body) => {
            collect_references_from_expression(condition, references);
            for statement in body {
                collect_references_from_ast_node(statement, references);
            }
        }

        NodeKind::Break | NodeKind::Continue | NodeKind::Operator(_) => {}
    }
}
