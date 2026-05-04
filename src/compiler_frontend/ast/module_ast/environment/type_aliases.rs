//! Type alias target resolution.
//!
//! WHAT: resolves `NamedType` placeholders inside type alias targets to concrete `DataType`s.
//! WHY: type aliases are compile-time-only type metadata; their targets must be fully resolved
//! before function signatures and struct fields are resolved.
//!
//! ## Cycle handling
//!
//! Type alias cycles (e.g. `A as B` + `B as A`) are detected by dependency sorting, because
//! `create_header` collects named-type dependency edges from alias targets just like from struct
//! fields and constant type annotations. Self-reference (`A as A`) also creates a self-loop edge.

use crate::compiler_frontend::ast::module_ast::environment::builder::AstModuleEnvironmentBuilder;
use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorMetaDataKey,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::declaration_syntax::type_syntax::resolve_type;
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::symbols::string_interning::StringTable;

impl<'context, 'services> AstModuleEnvironmentBuilder<'context, 'services> {
    /// Resolve all type alias targets in sorted-header order.
    ///
    /// WHAT: iterates sorted headers, resolving each `TypeAlias` target against already-resolved
    /// aliases and visible declarations.
    /// WHY: dependency sorting guarantees that when we reach an alias, all its dependencies have
    /// already been processed.
    pub(in crate::compiler_frontend::ast) fn resolve_type_aliases(
        &mut self,
        sorted_headers: &[Header],
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        for header in sorted_headers {
            let HeaderKind::TypeAlias { target, .. } = &header.kind else {
                continue;
            };
            let alias_path = &header.tokens.src_path;

            let visibility = self
                .import_environment
                .visibility_for(&header.source_file)
                .map_err(|error| self.error_messages(error, string_table))?;

            let resolved_target = {
                let type_resolution_context = self.type_resolution_context_for(visibility, None);

                resolve_type(
                    target,
                    &header.name_location,
                    &type_resolution_context,
                    string_table,
                )
                .map_err(|error| self.error_messages(error, string_table))?
            };

            // Reject aliases to external opaque types for Alpha.
            // WHAT: external types are opaque and cannot be aliased by user code.
            // WHY: aliases to opaque types would let user code pretend it owns a nominal type
            //     that it cannot construct or field-access, leading to confusing semantics.
            if let DataType::External { type_id } = &resolved_target {
                let type_name = self
                    .context
                    .external_package_registry
                    .get_type_by_id(*type_id)
                    .map(|def| def.name.to_string())
                    .unwrap_or_else(|| "external".to_string());
                let mut metadata = std::collections::HashMap::new();
                metadata.insert(
                    ErrorMetaDataKey::CompilationStage,
                    "AST Construction".to_string(),
                );
                metadata.insert(
                    ErrorMetaDataKey::PrimarySuggestion,
                    "Use the external type directly instead of creating a type alias".to_string(),
                );
                let error = CompilerError::new_rule_error_with_metadata(
                    format!(
                        "Cannot create a type alias for external type '{type_name}'. External types are opaque and cannot be aliased."
                    ),
                    header.name_location.clone(),
                    metadata,
                );
                return Err(self.error_messages(error, string_table));
            }

            self.resolved_type_aliases_by_path
                .insert(alias_path.to_owned(), resolved_target);
        }

        Ok(())
    }
}
