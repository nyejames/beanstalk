//! Shared template folding helpers for AST finalization.
//!
//! WHAT: Provides common template folding utilities used by both AST node
//! normalization and module constant normalization.
//!
//! WHY: Consolidates duplicated template folding logic to ensure consistent
//! behavior across all normalization contexts.

use crate::compiler_frontend::ast::templates::template::TemplateConstValueKind;
use crate::compiler_frontend::ast::templates::template_folding::TemplateFoldContext;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};

/// Folds a compile-time template into a `StringSlice` expression.
///
/// WHAT: Checks if the template is foldable (RenderableString or WrapperTemplate),
/// folds it using `TemplateFoldContext`, and returns a `StringId`.
///
/// WHY: This pattern is repeated in both AST node and module constant
/// normalization. Consolidating it ensures consistent folding behavior.
///
/// Returns `None` if the template is not foldable (NonConst or SlotInsertHelper).
pub(super) fn try_fold_template_to_string(
    template: &Template,
    source_file_scope: &InternedPath,
    path_format_config: &PathStringFormatConfig,
    project_path_resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Result<Option<StringId>, CompilerError> {
    match template.const_value_kind() {
        TemplateConstValueKind::RenderableString | TemplateConstValueKind::WrapperTemplate => {
            let mut fold_context = make_fold_context(
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
            );
            Ok(Some(template.fold_into_stringid(&mut fold_context)?))
        }
        TemplateConstValueKind::SlotInsertHelper | TemplateConstValueKind::NonConst => Ok(None),
    }
}

/// Creates a `TemplateFoldContext` from normalization parameters.
///
/// WHAT: Bundles the project-aware state required for template folding.
///
/// WHY: Avoids repeating this construction at every fold site.
pub(super) fn make_fold_context<'a>(
    source_file_scope: &'a InternedPath,
    path_format_config: &'a PathStringFormatConfig,
    project_path_resolver: &'a ProjectPathResolver,
    string_table: &'a mut StringTable,
) -> TemplateFoldContext<'a> {
    TemplateFoldContext {
        string_table,
        project_path_resolver,
        path_format_config,
        source_file_scope,
    }
}
