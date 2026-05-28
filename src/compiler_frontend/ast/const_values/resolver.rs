//! Const value resolution logic.
//!
//! WHAT: evaluates whether an AST expression resolves to a compile-time constant
//!       by substituting known const references and reusing the existing constant folder.
//! WHY: one shared resolver avoids duplicating fold/reference logic across config,
//!      AST finalization, and HIR metadata.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::const_values::facts::{
    AstConstDeclarationFact, ConstBindingScope, ConstBindingSource, ConstFactValueKind,
};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::optimizers::constant_folding::{ConstantFoldResult, constant_fold};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use rustc_hash::FxHashMap;

/// Lookup table for resolved const bindings visible in the current scope.
///
/// WHAT: maps interned declaration path to the fully resolved const expression.
/// WHY: reference resolution needs a narrow, explicit environment instead of
///      reaching into broader AST or scope context structures.
#[derive(Clone, Debug, Default)]
pub struct ConstValueEnvironment {
    bindings: FxHashMap<InternedPath, Expression>,
}

impl ConstValueEnvironment {
    /// Insert a resolved const binding into the environment.
    pub fn insert(&mut self, path: InternedPath, expression: Expression) {
        self.bindings.insert(path, expression);
    }

    /// Look up a const binding by path.
    pub fn lookup(&self, path: &InternedPath) -> Option<&Expression> {
        self.bindings.get(path)
    }
}

/// Reason why an expression could not be resolved to a compile-time constant.
///
/// WHAT: structured failure cases for const resolution.
/// WHY: callers decide how to report or ignore failures; the resolver does not
///      emit user-facing diagnostics directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstResolutionError {
    UnresolvedReference,
    NonConstReference,
    NonFoldableRuntimeExpression,
    CallInConstContext,
    MutableDeclaration,
    NonConstExpression,
}

/// Resolves AST expressions against a [`ConstValueEnvironment`] to determine
/// whether they are compile-time constants.
pub struct ConstValueResolver<'a> {
    string_table: &'a mut StringTable,
}

impl<'a> ConstValueResolver<'a> {
    pub fn new(string_table: &'a mut StringTable) -> Self {
        Self { string_table }
    }

    // ------------------------------
    //  Declaration resolution
    // ------------------------------

    /// Resolve an explicit `#=` top-level constant declaration.
    ///
    /// WHAT: explicit constants are const by syntax; this resolves their initializer
    ///       expression through the environment and builds a fact.
    pub fn resolve_explicit_top_level_constant(
        &mut self,
        declaration: &Declaration,
        environment: &ConstValueEnvironment,
    ) -> Result<AstConstDeclarationFact, ConstResolutionError> {
        let resolved = self.resolve_expression(&declaration.value, environment)?;

        Ok(AstConstDeclarationFact {
            declaration_path: declaration.id.clone(),
            scope: ConstBindingScope::ExplicitTopLevel,
            source: ConstBindingSource::ExplicitHash,
            value_kind: ConstFactValueKind::from_expression(&resolved),
            resolved_expression: resolved,
            location: declaration.value.location.clone(),
        })
    }

    /// Resolve a private inferred top-level declaration (`=` in start body).
    ///
    /// WHAT: mutable declarations are rejected; immutable declarations are const
    ///       only when their initializer fully resolves.
    pub fn resolve_private_top_level_declaration(
        &mut self,
        declaration: &Declaration,
        environment: &ConstValueEnvironment,
    ) -> Result<AstConstDeclarationFact, ConstResolutionError> {
        if declaration.value.value_mode.is_mutable() {
            return Err(ConstResolutionError::MutableDeclaration);
        }

        let resolved = self.resolve_expression(&declaration.value, environment)?;

        Ok(AstConstDeclarationFact {
            declaration_path: declaration.id.clone(),
            scope: ConstBindingScope::PrivateTopLevel,
            source: ConstBindingSource::InferredImmutable,
            value_kind: ConstFactValueKind::from_expression(&resolved),
            resolved_expression: resolved,
            location: declaration.value.location.clone(),
        })
    }

    /// Resolve a body-local private inferred declaration.
    ///
    /// WHAT: same rules as [`Self::resolve_private_top_level_declaration`] but
    ///       tagged with [`ConstBindingScope::BodyLocal`].
    pub fn resolve_body_local_declaration(
        &mut self,
        declaration: &Declaration,
        environment: &ConstValueEnvironment,
    ) -> Result<AstConstDeclarationFact, ConstResolutionError> {
        if declaration.value.value_mode.is_mutable() {
            return Err(ConstResolutionError::MutableDeclaration);
        }

        let resolved = self.resolve_expression(&declaration.value, environment)?;

        Ok(AstConstDeclarationFact {
            declaration_path: declaration.id.clone(),
            scope: ConstBindingScope::BodyLocal,
            source: ConstBindingSource::InferredImmutable,
            value_kind: ConstFactValueKind::from_expression(&resolved),
            resolved_expression: resolved,
            location: declaration.value.location.clone(),
        })
    }

    // ------------------------------
    //  Expression resolution
    // ------------------------------

    /// Resolve an arbitrary expression against the given environment.
    ///
    /// WHAT: the core resolution algorithm that handles literals, references,
    ///       runtime RPN, and coercion nodes.
    pub fn resolve_expression(
        &mut self,
        expression: &Expression,
        environment: &ConstValueEnvironment,
    ) -> Result<Expression, ConstResolutionError> {
        // Fast path: expressions that are already compile-time constants
        // (literals, composite collections, templates, etc.) need no substitution.
        if expression.is_compile_time_constant() {
            return Ok(expression.clone());
        }

        match &expression.kind {
            ExpressionKind::Reference(path) => self.resolve_reference(path, environment),

            ExpressionKind::Runtime(nodes) => self.resolve_runtime_rpn(nodes, environment),

            ExpressionKind::Coerced { value, .. } => {
                // A coercion does not change whether the inner value is const.
                self.resolve_expression(value, environment)
            }

            // Any call shape is treated as non-const. This includes function calls,
            // host calls, handled fallible calls, collection builtins, and method
            // calls (the latter two appear inside Runtime RPN, not as ExpressionKind).
            ExpressionKind::FunctionCall { .. }
            | ExpressionKind::HostFunctionCall { .. }
            | ExpressionKind::HandledFallibleFunctionCall { .. }
            | ExpressionKind::HandledFallibleHostFunctionCall { .. } => {
                Err(ConstResolutionError::CallInConstContext)
            }

            _ => Err(ConstResolutionError::NonConstExpression),
        }
    }

    // ------------------------------
    //  Internal helpers
    // ------------------------------

    fn resolve_reference(
        &mut self,
        path: &InternedPath,
        environment: &ConstValueEnvironment,
    ) -> Result<Expression, ConstResolutionError> {
        let resolved = environment
            .lookup(path)
            .ok_or(ConstResolutionError::UnresolvedReference)?;

        if resolved.is_compile_time_constant() {
            Ok(resolved.clone())
        } else {
            Err(ConstResolutionError::NonConstReference)
        }
    }

    /// Substitute known const references into an RPN stack, fold, and accept
    /// only when the result is a single compile-time expression.
    fn resolve_runtime_rpn(
        &mut self,
        nodes: &[AstNode],
        environment: &ConstValueEnvironment,
    ) -> Result<Expression, ConstResolutionError> {
        let mut substituted = Vec::with_capacity(nodes.len());

        for node in nodes {
            let new_node = match &node.kind {
                NodeKind::Rvalue(expression) => {
                    self.resolve_runtime_rvalue_node(expression, node, environment)?
                }
                _ => node.clone(),
            };
            substituted.push(new_node);
        }

        match constant_fold(&substituted, self.string_table)
            .map_err(|_| ConstResolutionError::NonFoldableRuntimeExpression)?
        {
            ConstantFoldResult::Unchanged => {
                Err(ConstResolutionError::NonFoldableRuntimeExpression)
            }
            ConstantFoldResult::Folded(stack) => {
                if stack.len() == 1
                    && let NodeKind::Rvalue(expression) = &stack[0].kind
                    && expression.is_compile_time_constant()
                {
                    return Ok(expression.clone());
                }
                Err(ConstResolutionError::NonFoldableRuntimeExpression)
            }
        }
    }

    fn resolve_runtime_rvalue_node(
        &mut self,
        expression: &Expression,
        original_node: &AstNode,
        environment: &ConstValueEnvironment,
    ) -> Result<AstNode, ConstResolutionError> {
        let resolved = match &expression.kind {
            ExpressionKind::Reference(..) | ExpressionKind::Coerced { .. } => {
                Some(self.resolve_expression(expression, environment)?)
            }
            _ => None,
        };

        if let Some(resolved_expression) = resolved {
            return Ok(AstNode {
                kind: NodeKind::Rvalue(resolved_expression),
                location: original_node.location.clone(),
                scope: original_node.scope.clone(),
            });
        }

        Ok(original_node.clone())
    }
}
