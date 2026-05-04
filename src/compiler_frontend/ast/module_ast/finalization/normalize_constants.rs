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
use super::template_helpers::try_fold_template_to_string;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::ast::templates::template::TemplateConstValueKind;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::string_interning::StringTable;

impl AstFinalizer<'_, '_> {
    /// Normalizes module constants for HIR metadata.
    ///
    /// WHAT: Folds compile-time templates in module constants and filters
    /// out composition-only helpers like `SlotInsertHelper`.
    ///
    /// WHY: Module constants are HIR metadata, not runtime statements. They
    /// must be fully foldable and have stable backend-facing shapes.
    pub(crate) fn normalize_module_constants_for_hir(
        &self,
        project_path_resolver: &ProjectPathResolver,
        string_table: &mut StringTable,
    ) -> Result<Vec<Declaration>, CompilerError> {
        let mut normalized_constants = Vec::with_capacity(self.environment.module_constants.len());

        for declaration in &self.environment.module_constants {
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
    ) -> Result<Expression, CompilerError> {
        increment_ast_counter(AstCounter::ModuleConstantNormalizationExpressionsVisited);

        let mut normalized = expression.to_owned();
        normalized.kind = match &expression.kind {
            ExpressionKind::Template(template) => match template.const_value_kind() {
                TemplateConstValueKind::RenderableString
                | TemplateConstValueKind::WrapperTemplate => {
                    let folded = try_fold_template_to_string(
                        template,
                        source_file_scope,
                        &self.context.path_format_config,
                        project_path_resolver,
                        string_table,
                    )?
                    .expect("RenderableString/WrapperTemplate should always fold");
                    normalized.data_type = DataType::StringSlice;
                    ExpressionKind::StringSlice(folded)
                }

                TemplateConstValueKind::SlotInsertHelper => expression.kind.to_owned(),

                TemplateConstValueKind::NonConst => {
                    return Err(CompilerError::compiler_error(
                        "Non-constant template reached AST finalization in module constant metadata.",
                    ));
                }
            },

            ExpressionKind::Collection(items) => ExpressionKind::Collection(
                items
                    .iter()
                    .map(|item| {
                        self.normalize_module_constant_expression(
                            item,
                            source_file_scope,
                            project_path_resolver,
                            string_table,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),

            ExpressionKind::StructInstance(fields) => ExpressionKind::StructInstance(
                fields
                    .iter()
                    .map(|field| {
                        Ok(Declaration {
                            id: field.id.to_owned(),
                            value: self.normalize_module_constant_expression(
                                &field.value,
                                source_file_scope,
                                project_path_resolver,
                                string_table,
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>, CompilerError>>()?,
            ),

            ExpressionKind::Range(start, end) => ExpressionKind::Range(
                Box::new(self.normalize_module_constant_expression(
                    start,
                    source_file_scope,
                    project_path_resolver,
                    string_table,
                )?),
                Box::new(self.normalize_module_constant_expression(
                    end,
                    source_file_scope,
                    project_path_resolver,
                    string_table,
                )?),
            ),

            ExpressionKind::ResultConstruct { variant, value } => ExpressionKind::ResultConstruct {
                variant: *variant,
                value: Box::new(self.normalize_module_constant_expression(
                    value,
                    source_file_scope,
                    project_path_resolver,
                    string_table,
                )?),
            },

            ExpressionKind::Coerced { value, to_type } => ExpressionKind::Coerced {
                value: Box::new(self.normalize_module_constant_expression(
                    value,
                    source_file_scope,
                    project_path_resolver,
                    string_table,
                )?),
                to_type: to_type.to_owned(),
            },

            _ => expression.kind.to_owned(),
        };
        Ok(normalized)
    }
}

// --------------------------
//  Helper-only template filter
// --------------------------

fn contains_helper_only_template_value(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Template(template) => matches!(
            template.const_value_kind(),
            TemplateConstValueKind::SlotInsertHelper
        ),

        ExpressionKind::Collection(items) => items.iter().any(contains_helper_only_template_value),

        ExpressionKind::StructInstance(fields) => fields
            .iter()
            .any(|field| contains_helper_only_template_value(&field.value)),

        ExpressionKind::Range(start, end) => {
            contains_helper_only_template_value(start) || contains_helper_only_template_value(end)
        }

        ExpressionKind::ResultConstruct { value, .. } => contains_helper_only_template_value(value),

        ExpressionKind::Coerced { value, .. } => contains_helper_only_template_value(value),

        _ => false,
    }
}
