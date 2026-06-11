//! Const fact collection during AST finalization.
//!
//! WHAT: walks the finalized AST and collects const facts for explicit module
//!       constants, private inferred top-level start-body declarations, and
//!       body-local declarations.
//! WHY: separates the detailed walking logic from the main finalizer
//!      orchestration to keep `finalizer.rs` readable.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::const_values::facts::AstConstFacts;
use crate::compiler_frontend::ast::const_values::resolver::{
    ConstValueEnvironment, ConstValueResolver,
};
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::expression_types::FallibleHandling;
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::statements::value_production::types::ValueBlock;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

/// Collects const facts from the finalized AST after normalization.
pub(super) struct ConstFactCollector<'a> {
    resolver: ConstValueResolver<'a>,
    facts: AstConstFacts,
    module_explicit_env: ConstValueEnvironment,
}

impl<'a> ConstFactCollector<'a> {
    pub(super) fn new(string_table: &'a mut StringTable) -> Self {
        Self {
            resolver: ConstValueResolver::new(string_table),
            facts: AstConstFacts::default(),
            module_explicit_env: ConstValueEnvironment::default(),
        }
    }

    /// Collect const facts from module constants and AST nodes.
    ///
    /// WHAT: resolves explicit module constants first (so private and body-local
    ///       facts can reference them), then walks the start function body for
    ///       private top-level facts, then walks all other function bodies for
    ///       body-local facts.
    pub(super) fn collect(
        mut self,
        module_constants: &[Declaration],
        ast_nodes: &[AstNode],
        start_function_path: &InternedPath,
    ) -> AstConstFacts {
        self.collect_explicit_top_level_facts(module_constants);
        self.collect_private_and_body_local_facts(ast_nodes, start_function_path);
        self.facts
    }

    // ------------------------------
    //  Explicit top-level constants
    // ------------------------------

    /// Resolve explicit module constants and register them as facts.
    fn collect_explicit_top_level_facts(&mut self, module_constants: &[Declaration]) {
        for declaration in module_constants {
            match self
                .resolver
                .resolve_explicit_top_level_constant(declaration, &self.module_explicit_env)
            {
                Ok(fact) => {
                    self.module_explicit_env
                        .insert(declaration.id.clone(), fact.resolved_expression.clone());
                    self.facts.declarations.insert(declaration.id.clone(), fact);
                }

                Err(_) => {
                    // Explicit constants that fail resolution are skipped silently.
                    // They were already validated earlier; this is a safety fallback.
                }
            }
        }
    }

    // ------------------------------------
    //  Private top-level and body-local
    // ------------------------------------

    /// Walk AST nodes to collect private top-level and body-local const facts.
    fn collect_private_and_body_local_facts(
        &mut self,
        ast_nodes: &[AstNode],
        start_function_path: &InternedPath,
    ) {
        for node in ast_nodes {
            if let NodeKind::Function(path, _, body) = &node.kind {
                if path == start_function_path {
                    let mut start_env = self.module_explicit_env.clone();
                    self.walk_start_body(body, &mut start_env);
                } else {
                    let mut function_env = self.module_explicit_env.clone();
                    self.walk_body_local(body, &mut function_env);
                }
            }
        }
    }

    // ------------------------------
    //  Start body walker
    // ------------------------------

    /// Walk the start function body.
    ///
    /// WHAT: direct children that are variable declarations become
    ///       `PrivateTopLevel` facts. Nested scopes are walked for `BodyLocal`
    ///       facts. Declarations that do not resolve as const are skipped
    ///       silently.
    fn walk_start_body(&mut self, nodes: &[AstNode], env: &mut ConstValueEnvironment) {
        for node in nodes {
            match &node.kind {
                NodeKind::VariableDeclaration(declaration) => {
                    self.walk_expression_for_body_local(&declaration.value, env);
                    self.try_add_private_top_level_fact(declaration, env);
                }

                _ => {
                    self.walk_node_for_body_local(node, env);
                }
            }
        }
    }

    /// Attempt to resolve a start-body declaration as a private top-level const fact.
    ///
    /// WHAT: on success, inserts the fact into both the local environment
    ///       and the output fact table.
    fn try_add_private_top_level_fact(
        &mut self,
        declaration: &Declaration,
        env: &mut ConstValueEnvironment,
    ) {
        match self
            .resolver
            .resolve_private_top_level_declaration(declaration, env)
        {
            Ok(fact) => {
                env.insert(declaration.id.clone(), fact.resolved_expression.clone());
                self.facts.declarations.insert(declaration.id.clone(), fact);
            }

            Err(_) => {
                // Not a const fact — skip silently. Mutable declarations,
                // forward references, and runtime expressions are all
                // intentionally omitted.
            }
        }
    }

    // ------------------------------
    //  Body-local walker
    // ------------------------------

    /// Walk a function body for body-local const facts.
    ///
    /// WHAT: all variable declarations inside function bodies (and nested
    ///       scopes) are attempted as `BodyLocal` facts. Each nested scope
    ///       receives a cloned environment so declarations do not leak outward.
    fn walk_body_local(&mut self, nodes: &[AstNode], env: &mut ConstValueEnvironment) {
        for node in nodes {
            self.walk_node_for_body_local(node, env);
        }
    }

    /// Walk a single AST node for body-local const facts.
    ///
    /// WHAT: dispatches over all [`NodeKind`] variants, cloning the environment
    ///       for nested scopes and attempting to register const declarations.
    fn walk_node_for_body_local(&mut self, node: &AstNode, env: &mut ConstValueEnvironment) {
        match &node.kind {
            NodeKind::VariableDeclaration(declaration) => {
                self.walk_expression_for_body_local(&declaration.value, env);
                self.try_add_body_local_fact(declaration, env);
            }

            NodeKind::ScopedBlock { body } => {
                let mut nested_env = env.clone();
                self.walk_body_local(body, &mut nested_env);
            }

            NodeKind::If(condition, then_body, else_body) => {
                self.walk_expression_for_body_local(condition, env);

                let mut then_env = env.clone();
                self.walk_body_local(then_body, &mut then_env);

                if let Some(else_body) = else_body {
                    let mut else_env = env.clone();
                    self.walk_body_local(else_body, &mut else_env);
                }
            }

            NodeKind::Assert {
                condition,
                message: _,
            } => {
                self.walk_expression_for_body_local(condition, env);
            }

            NodeKind::Match {
                scrutinee,
                arms,
                default,
                ..
            } => {
                self.walk_expression_for_body_local(scrutinee, env);

                for arm in arms {
                    self.walk_match_pattern_for_body_local(&arm.pattern, env);
                    if let Some(guard) = &arm.guard {
                        self.walk_expression_for_body_local(guard, env);
                    }

                    let mut arm_env = env.clone();
                    self.walk_body_local(&arm.body, &mut arm_env);
                }

                if let Some(default_body) = default {
                    let mut default_env = env.clone();
                    self.walk_body_local(default_body, &mut default_env);
                }
            }

            NodeKind::RangeLoop { range, body, .. } => {
                self.walk_expression_for_body_local(&range.start, env);
                self.walk_expression_for_body_local(&range.end, env);
                if let Some(step) = &range.step {
                    self.walk_expression_for_body_local(step, env);
                }

                let mut loop_env = env.clone();
                self.walk_body_local(body, &mut loop_env);
            }

            NodeKind::CollectionLoop { iterable, body, .. } => {
                self.walk_expression_for_body_local(iterable, env);

                let mut loop_env = env.clone();
                self.walk_body_local(body, &mut loop_env);
            }

            NodeKind::WhileLoop(condition, body) => {
                self.walk_expression_for_body_local(condition, env);

                let mut loop_env = env.clone();
                self.walk_body_local(body, &mut loop_env);
            }

            NodeKind::Function(_, _, body) => {
                let mut nested_env = env.clone();
                self.walk_body_local(body, &mut nested_env);
            }

            NodeKind::Return(expressions) => {
                self.walk_expressions_for_body_local(expressions, env);
            }

            NodeKind::ThenValue(produced_values) => {
                self.walk_expressions_for_body_local(&produced_values.expressions, env);
            }

            NodeKind::ReturnError(expression) | NodeKind::PushStartRuntimeFragment(expression) => {
                self.walk_expression_for_body_local(expression, env);
            }

            NodeKind::FieldAccess { base, .. } => {
                self.walk_node_for_body_local(base, env);
            }

            NodeKind::MethodCall { receiver, args, .. } => {
                self.walk_node_for_body_local(receiver, env);
                self.walk_call_arguments_for_body_local(args, env);
            }

            NodeKind::CollectionBuiltinCall { receiver, args, .. }
            | NodeKind::MapBuiltinCall { receiver, args, .. } => {
                self.walk_node_for_body_local(receiver, env);
                self.walk_call_arguments_for_body_local(args, env);
            }

            NodeKind::FunctionCall { args, .. } | NodeKind::HostFunctionCall { args, .. } => {
                self.walk_call_arguments_for_body_local(args, env);
            }

            NodeKind::HandledFallibleFunctionCall { args, handling, .. }
            | NodeKind::HandledFallibleHostFunctionCall { args, handling, .. } => {
                self.walk_call_arguments_for_body_local(args, env);
                self.walk_fallible_handling_for_body_local(handling, env);
            }

            NodeKind::Assignment { target, value } => {
                self.walk_node_for_body_local(target, env);
                self.walk_expression_for_body_local(value, env);
            }

            NodeKind::MultiBind { value, .. } | NodeKind::Rvalue(value) => {
                self.walk_expression_for_body_local(value, env);
            }

            NodeKind::StructDefinition(_, fields) => {
                for field in fields {
                    self.walk_expression_for_body_local(&field.value, env);
                }
            }

            // All other node kinds do not contain declarations or nested
            // bodies that need walking for const facts.
            NodeKind::Break | NodeKind::Continue | NodeKind::Operator(_) => {}
        }
    }

    /// Attempt to resolve a body-local declaration as a const fact.
    ///
    /// WHAT: on success, inserts the fact into both the local environment
    ///       and the output fact table.
    fn try_add_body_local_fact(
        &mut self,
        declaration: &Declaration,
        env: &mut ConstValueEnvironment,
    ) {
        match self
            .resolver
            .resolve_body_local_declaration(declaration, env)
        {
            Ok(fact) => {
                env.insert(declaration.id.clone(), fact.resolved_expression.clone());
                self.facts.declarations.insert(declaration.id.clone(), fact);
            }

            Err(_) => {
                // Not a const fact — skip silently.
            }
        }
    }

    /// Walk call arguments for body-local const facts.
    fn walk_call_arguments_for_body_local(
        &mut self,
        arguments: &[CallArgument],
        env: &mut ConstValueEnvironment,
    ) {
        for argument in arguments {
            self.walk_expression_for_body_local(&argument.value, env);
        }
    }

    /// Walk a list of expressions for body-local const facts.
    fn walk_expressions_for_body_local(
        &mut self,
        expressions: &[Expression],
        env: &mut ConstValueEnvironment,
    ) {
        for expression in expressions {
            self.walk_expression_for_body_local(expression, env);
        }
    }

    /// Walk a match pattern for body-local const facts.
    ///
    /// WHAT: only literal, option-value, and relational patterns contain
    ///       nested expressions that need walking.
    fn walk_match_pattern_for_body_local(
        &mut self,
        pattern: &MatchPattern,
        env: &mut ConstValueEnvironment,
    ) {
        match pattern {
            MatchPattern::Literal(expression) => {
                self.walk_expression_for_body_local(expression, env);
            }

            MatchPattern::OptionValue { value, .. } | MatchPattern::Relational { value, .. } => {
                self.walk_expression_for_body_local(value, env);
            }

            MatchPattern::OptionNone { .. }
            | MatchPattern::Wildcard { .. }
            | MatchPattern::ChoiceVariant { .. }
            | MatchPattern::Capture { .. }
            | MatchPattern::OptionPresentCapture { .. } => {}
        }
    }

    /// Walk an expression tree for body-local const facts.
    ///
    /// WHAT: recursively descends through nested AST nodes, function literals,
    ///       and fallible handling structures.
    /// WHY: expressions may contain scoped blocks or call arguments that
    ///      reference or declare const-foldable values.
    fn walk_expression_for_body_local(
        &mut self,
        expression: &Expression,
        env: &mut ConstValueEnvironment,
    ) {
        match &expression.kind {
            ExpressionKind::Runtime(nodes) => {
                for node in nodes {
                    self.walk_node_for_body_local(node, env);
                }
            }

            ExpressionKind::Copy(node) => {
                self.walk_node_for_body_local(node, env);
            }

            ExpressionKind::Function(_, body) => {
                let mut nested_env = env.clone();
                self.walk_body_local(body, &mut nested_env);
            }

            ExpressionKind::FunctionCall { args, .. }
            | ExpressionKind::HostFunctionCall { args, .. } => {
                self.walk_call_arguments_for_body_local(args, env);
            }

            ExpressionKind::HandledFallibleFunctionCall { args, handling, .. }
            | ExpressionKind::HandledFallibleHostFunctionCall { args, handling, .. } => {
                self.walk_call_arguments_for_body_local(args, env);
                self.walk_fallible_handling_for_body_local(handling, env);
            }

            ExpressionKind::HandledFallibleExpression { value, handling } => {
                self.walk_expression_for_body_local(value, env);
                self.walk_fallible_handling_for_body_local(handling, env);
            }

            ExpressionKind::BuiltinCast { value, .. }
            | ExpressionKind::FallibleCarrierConstruct { value, .. }
            | ExpressionKind::OptionPropagation { value }
            | ExpressionKind::Coerced { value, .. } => {
                self.walk_expression_for_body_local(value, env);
            }

            ExpressionKind::Collection(items) => {
                self.walk_expressions_for_body_local(items, env);
            }

            ExpressionKind::MapLiteral(entries) => {
                for entry in entries {
                    self.walk_expression_for_body_local(&entry.key, env);
                    self.walk_expression_for_body_local(&entry.value, env);
                }
            }

            ExpressionKind::StructDefinition(fields) | ExpressionKind::StructInstance(fields) => {
                for field in fields {
                    self.walk_expression_for_body_local(&field.value, env);
                }
            }

            ExpressionKind::ChoiceConstruct { fields, .. } => {
                for field in fields {
                    self.walk_expression_for_body_local(&field.value, env);
                }
            }

            ExpressionKind::Range(start, end) => {
                self.walk_expression_for_body_local(start, env);
                self.walk_expression_for_body_local(end, env);
            }

            ExpressionKind::ValueBlock { block } => match block.as_ref() {
                ValueBlock::If(value_if) => {
                    self.walk_expression_for_body_local(&value_if.condition, env);

                    let mut then_env = env.clone();
                    self.walk_body_local(&value_if.then_body, &mut then_env);

                    let mut else_env = env.clone();
                    self.walk_body_local(&value_if.else_body, &mut else_env);
                }
                ValueBlock::Match(value_match) => {
                    self.walk_expression_for_body_local(&value_match.scrutinee, env);

                    for arm in &value_match.arms {
                        if let Some(guard) = &arm.guard {
                            self.walk_expression_for_body_local(guard, env);
                        }
                        let mut arm_env = env.clone();
                        self.walk_body_local(&arm.body, &mut arm_env);
                    }

                    if let Some(default_body) = &value_match.default {
                        let mut default_env = env.clone();
                        self.walk_body_local(default_body, &mut default_env);
                    }
                }
                ValueBlock::Catch(value_catch) => {
                    self.walk_expression_for_body_local(&value_catch.handled_value, env);
                }
            },

            // Terminal expression kinds carry no nested structure to walk.
            ExpressionKind::NoValue
            | ExpressionKind::OptionNone
            | ExpressionKind::Int(_)
            | ExpressionKind::Float(_)
            | ExpressionKind::StringSlice(_)
            | ExpressionKind::Bool(_)
            | ExpressionKind::Char(_)
            | ExpressionKind::Path(_)
            | ExpressionKind::Reference(_)
            | ExpressionKind::Template(_) => {}
        }
    }

    /// Walk fallible handler bodies for body-local const facts.
    ///
    /// WHAT: walks the handler body in an isolated environment.
    fn walk_fallible_handling_for_body_local(
        &mut self,
        handling: &FallibleHandling,
        env: &mut ConstValueEnvironment,
    ) {
        let FallibleHandling::Handler { body, .. } = handling else {
            return;
        };

        let mut handler_env = env.clone();
        self.walk_body_local(body, &mut handler_env);
    }
}
