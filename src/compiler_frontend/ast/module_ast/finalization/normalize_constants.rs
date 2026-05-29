//! Module constant normalization for HIR metadata preparation.
//!
//! WHAT: Normalizes module constant expressions by folding compile-time
//! templates and recursively processing collections and struct instances.
//! Returns normalized constants as HIR metadata, not runtime statements.
//!
//! WHY: Module constants are compile-time metadata that HIR exposes for
//! tooling and codegen decisions. They require separate normalization from
//! AST nodes because they must be fully foldable at compile time.
//!
//! ## Difference from AST Node Normalization
//!
//! **AST Node Normalization**:
//! - Mutates nodes in place
//! - Handles both constant and runtime templates
//! - Preserves runtime template structure for HIR lowering
//!
//! **Module Constant Normalization**:
//! - Returns new normalized expressions
//! - Only handles compile-time foldable templates
//! - Rejects non-constant templates as compiler errors
//! - Filters out `SlotInsertHelper` constants (composition-only)
//!
//! ## SlotInsertHelper Filtering
//!
//! `$insert(..)` helper constants only exist for AST template composition.
//! They don't have stable backend-facing value shapes, so HIR must not
//! receive them as module constants. They are filtered during normalization.

use super::finalizer::AstFinalizer;
use super::normalize_ast::TemplateNormalizationError;
use super::template_helpers::try_fold_template_to_string;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::ast::templates::template::TemplateConstValueKind;
use crate::compiler_frontend::ast::templates::template_control_flow::validate_const_required_template_control_flow;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::string_interning::StringTable;

impl AstFinalizer<'_, '_> {
    /// Normalizes module constants for HIR.
    ///
    /// WHAT: Folds compile-time templates in module constants and filters
    /// out composition-only helpers like `SlotInsertHelper`.
    ///
    /// WHY: Module constants are HIR metadata, not runtime statements. They
    /// must be fully foldable and have stable backend-facing shapes.
    pub(super) fn normalize_module_constants_for_hir(
        &self,
        project_path_resolver: &ProjectPathResolver,
        string_table: &mut StringTable,
    ) -> Result<Vec<Declaration>, TemplateNormalizationError> {
        let mut normalized_constants =
            Vec::with_capacity(self.environment.lookups.module_constants.len());

        for declaration in &self.environment.lookups.module_constants {
            // `$insert(..)` helper constants only exist so AST template composition can
            // splice them into an immediate parent wrapper. They do not have a stable
            // backend-facing value shape, so HIR must not receive them as module consts.
            // Wrapper constants remain valid here even when their authored source used
            // slot-oriented composition structure, as long as the final constant value
            // classifies as `RenderableString` or `WrapperTemplate`.
            if contains_helper_only_template_value(&declaration.value) {
                continue;
            }

            let source_file_scope = self
                .environment
                .lookups
                .module_symbols
                .canonical_source_by_symbol_path
                .get(&declaration.id)
                .unwrap_or(&declaration.value.location.scope);

            normalized_constants.push(Declaration {
                id: declaration.id.to_owned(),
                value: self.normalize_module_constant_expression(
                    &declaration.value,
                    source_file_scope,
                    project_path_resolver,
                    string_table,
                )?,
            });
        }

        Ok(normalized_constants)
    }

    /// Recursively normalizes a module constant expression.
    ///
    /// WHAT: Folds compile-time templates and recursively processes collections,
    /// struct instances, ranges, and result constructs.
    ///
    /// WHY: Module constants can contain nested structures that must all be
    /// normalized to ensure HIR receives fully folded metadata.
    ///
    /// Returns a new normalized expression (does not mutate the input).
    fn normalize_module_constant_expression(
        &self,
        expression: &Expression,
        source_file_scope: &InternedPath,
        project_path_resolver: &ProjectPathResolver,
        string_table: &mut StringTable,
    ) -> Result<Expression, TemplateNormalizationError> {
        increment_ast_counter(AstCounter::ModuleConstantNormalizationExpressionsVisited);

        // Shorthand for the recursive case so each arm reads as a single step.
        let mut normalize_expr =
            |expr: &Expression| -> Result<Expression, TemplateNormalizationError> {
                self.normalize_module_constant_expression(
                    expr,
                    source_file_scope,
                    project_path_resolver,
                    string_table,
                )
            };

        let mut normalized = expression.to_owned();
        normalized.kind = match &expression.kind {
            ExpressionKind::Template(template) => {
                validate_const_required_template_control_flow(template, &template.location)?;

                match template.const_value_kind() {
                    TemplateConstValueKind::RenderableString
                    | TemplateConstValueKind::WrapperTemplate => {
                        let Some(folded) = try_fold_template_to_string(
                            template,
                            source_file_scope,
                            &self.context.path_format_config,
                            project_path_resolver,
                            string_table,
                            self.context.template_const_loop_iteration_limit,
                        )?
                        else {
                            return Err(CompilerError::compiler_error(
                                "Foldable module-constant template did not produce a folded string.",
                            )
                            .into());
                        };
                        normalized.diagnostic_type = DataType::StringSlice;
                        ExpressionKind::StringSlice(folded)
                    }

                    // Preserve helper templates so wrapper composition can still reference them.
                    TemplateConstValueKind::SlotInsertHelper => expression.kind.to_owned(),

                    TemplateConstValueKind::NonConst => {
                        return Err(CompilerError::compiler_error(
                            "Non-constant template reached AST finalization in module constant metadata.",
                        )
                        .into());
                    }
                }
            }

            ExpressionKind::Collection(items) => ExpressionKind::Collection(
                items
                    .iter()
                    .map(normalize_expr)
                    .collect::<Result<Vec<_>, _>>()?,
            ),

            ExpressionKind::StructInstance(fields) => ExpressionKind::StructInstance(
                fields
                    .iter()
                    .map(|field| {
                        Ok(Declaration {
                            id: field.id.to_owned(),
                            value: normalize_expr(&field.value)?,
                        })
                    })
                    .collect::<Result<Vec<_>, TemplateNormalizationError>>()?,
            ),

            ExpressionKind::Range(start, end) => ExpressionKind::Range(
                Box::new(normalize_expr(start)?),
                Box::new(normalize_expr(end)?),
            ),

            ExpressionKind::FallibleCarrierConstruct { variant, value } => {
                ExpressionKind::FallibleCarrierConstruct {
                    variant: *variant,
                    value: Box::new(normalize_expr(value)?),
                }
            }

            ExpressionKind::OptionPropagation { value } => ExpressionKind::OptionPropagation {
                value: Box::new(normalize_expr(value)?),
            },

            ExpressionKind::Coerced { value, to_type } => ExpressionKind::Coerced {
                value: Box::new(normalize_expr(value)?),
                to_type: *to_type,
            },

            // Leaf expressions (literals, identifiers, operators, etc.) need no recursion.
            _ => expression.kind.to_owned(),
        };
        Ok(normalized)
    }
}

// --------------------------
//  Helper-only template filter
// --------------------------

fn contains_helper_only_template_value(expression: &Expression) -> bool {
    // Shorthand so composite checks read as a single step.
    let check = |expr: &Expression| contains_helper_only_template_value(expr);

    match &expression.kind {
        ExpressionKind::Template(template) => matches!(
            template.const_value_kind(),
            TemplateConstValueKind::SlotInsertHelper
        ),

        ExpressionKind::Collection(items) => items.iter().any(check),

        ExpressionKind::StructInstance(fields) => fields.iter().any(|field| check(&field.value)),

        ExpressionKind::Range(start, end) => check(start) || check(end),

        ExpressionKind::FallibleCarrierConstruct { value, .. }
        | ExpressionKind::OptionPropagation { value } => check(value),

        ExpressionKind::Coerced { value, .. } => check(value),

        _ => false,
    }
}
