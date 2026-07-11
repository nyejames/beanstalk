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
use super::template_helpers::{TemplateFinalizationFoldInputs, try_fold_template_to_string};
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::const_values::resolver::classify_template_from_effective_tir;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::template::{TemplateConstValueKind, TemplateType};
use crate::compiler_frontend::ast::templates::template_control_flow::validate_const_required_template_control_flow;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use std::rc::Rc;

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
            if self.contains_helper_only_template_value(&declaration.value, string_table)? {
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
                validate_const_required_template_control_flow(
                    template,
                    &self.context.template_ir_registry.borrow(),
                    string_table,
                )
                .map_err(|diagnostic| {
                    TemplateNormalizationError::Diagnostic(Box::new(diagnostic))
                })?;

                let fold_result = try_fold_template_to_string(
                    template,
                    TemplateFinalizationFoldInputs {
                        source_file_scope,
                        path_format_config: &self.context.path_format_config,
                        project_path_resolver,
                        string_table,
                        template_const_loop_iteration_limit: self
                            .context
                            .template_const_loop_iteration_limit,
                        template_ir_store: &self.context.template_ir_store,
                        template_ir_registry: Rc::clone(&self.context.template_ir_registry),
                    },
                )?;

                if let Some(folded) = fold_result.folded {
                    normalized.diagnostic_type = DataType::StringSlice;
                    ExpressionKind::StringSlice(folded)
                } else {
                    match fold_result.const_value_kind {
                        TemplateConstValueKind::LoopControlSignal
                        | TemplateConstValueKind::SlotInsertHelper => expression.kind.to_owned(),

                        TemplateConstValueKind::RenderableString
                        | TemplateConstValueKind::WrapperTemplate => {
                            return Err(CompilerError::compiler_error(
                                "Foldable module-constant template did not produce a folded string.",
                            )
                            .into());
                        }

                        TemplateConstValueKind::NonConst => {
                            return Err(CompilerError::compiler_error(
                                "Non-constant template reached AST finalization in module constant metadata.",
                            )
                            .into());
                        }
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

            #[cfg(test)]
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

impl AstFinalizer<'_, '_> {
    fn contains_helper_only_template_value(
        &self,
        expression: &Expression,
        string_table: &StringTable,
    ) -> Result<bool, TemplateNormalizationError> {
        let contains_helper = match &expression.kind {
            ExpressionKind::Template(template) => {
                if !matches!(template.kind, TemplateType::SlotInsert(_)) {
                    return Ok(false);
                }

                let template_const_kind = classify_template_from_effective_tir(
                    template,
                    &self.context.template_ir_registry,
                    string_table,
                )?;
                matches!(
                    template_const_kind,
                    TemplateConstValueKind::SlotInsertHelper
                )
            }

            ExpressionKind::Collection(items) => {
                for item in items {
                    if self.contains_helper_only_template_value(item, string_table)? {
                        return Ok(true);
                    }
                }
                false
            }

            ExpressionKind::StructInstance(fields)
            | ExpressionKind::ChoiceConstruct { fields, .. } => {
                for field in fields {
                    if self.contains_helper_only_template_value(&field.value, string_table)? {
                        return Ok(true);
                    }
                }
                false
            }

            ExpressionKind::Range(start, end) => {
                self.contains_helper_only_template_value(start, string_table)?
                    || self.contains_helper_only_template_value(end, string_table)?
            }

            #[cfg(test)]
            ExpressionKind::FallibleCarrierConstruct { value, .. } => {
                self.contains_helper_only_template_value(value, string_table)?
            }

            ExpressionKind::OptionPropagation { value } => {
                self.contains_helper_only_template_value(value, string_table)?
            }

            ExpressionKind::Coerced { value, .. } => {
                self.contains_helper_only_template_value(value, string_table)?
            }

            _ => false,
        };

        Ok(contains_helper)
    }
}
