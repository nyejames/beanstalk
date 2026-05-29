//! Required service accessors for AST scope contexts.
//!
//! ## Diagnostic boundary
//!
//! `CompilerError` in this module means a missing compiler setup service or internal
//! infrastructure failure. These are not user-facing diagnostics.

use super::*;

impl ScopeContext {
    // --------------------------
    //  Required services
    // --------------------------

    pub(crate) fn required_project_path_resolver(
        &self,
        operation: &str,
    ) -> Result<&ProjectPathResolver, CompilerError> {
        let Some(resolver) = self.shared.project_path_resolver.as_ref() else {
            return_compiler_error!(
                "Missing project path resolver during '{}'. Context scope: '{:?}'. This is a compiler setup bug.",
                operation,
                self.scope
            );
        };
        Ok(resolver)
    }

    pub(crate) fn required_source_file_scope(
        &self,
        operation: &str,
    ) -> Result<&InternedPath, CompilerError> {
        let Some(source_scope) = self.shared.source_file_scope.as_ref() else {
            return_compiler_error!(
                "Missing source file scope during '{}'. Context scope: '{:?}'. This is a compiler setup bug.",
                operation,
                self.scope
            );
        };
        Ok(source_scope)
    }

    /// Build a [`TemplateFoldContext`] from the current scope's shared services.
    ///
    /// WHAT: gathers the path resolver, source file scope, and format config needed
    ///       to fold template expressions at compile time.
    /// WHY: template folding happens in several parser paths (body parser, expression
    ///      parser, top-level const folding); this keeps the context assembly in one place.
    pub fn new_template_fold_context<'b>(
        &'b self,
        string_table: &'b mut StringTable,
        operation: &str,
    ) -> Result<TemplateFoldContext<'b>, CompilerError> {
        let resolver = self.required_project_path_resolver(operation)?;
        let source_file_scope = self.required_source_file_scope(operation)?;
        Ok(TemplateFoldContext {
            string_table,
            project_path_resolver: resolver,
            path_format_config: &self.path_format_config,
            source_file_scope,
            template_const_loop_iteration_limit: self.shared.template_const_loop_iteration_limit,
            bindings: Vec::new(),
        })
    }
}
