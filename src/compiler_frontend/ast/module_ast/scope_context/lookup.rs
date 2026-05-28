//! Visible source and external symbol lookup for AST scope contexts.
//!
//! WHAT: provides `ScopeContext` methods that resolve names to local declarations,
//! source-visible symbols, receiver methods, and external package items.
//! WHY: expression and type parsing need a single, consistent lookup surface that
//! respects file-local visibility, import aliases, and receiver-call boundaries.
//!
//! All lookups in this file are read-only with respect to `ScopeContext` state.
//! Mutations to local declarations and visibility live in sibling modules
//! (`builders`, `local_declarations`, `diagnostic_sinks`).

use super::*;
use crate::compiler_frontend::ast::generic_functions::GenericFunctionTemplate;

impl ScopeContext {
    // --------------------------
    //  Symbol lookup
    // --------------------------

    pub(crate) fn get_reference(&self, name: &StringId) -> Option<&Declaration> {
        // 1. Locals (latest visible local wins)
        if let Some(indices) = self.local_declarations_by_name.get(name) {
            return indices
                .last()
                .map(|index| &self.local_declarations[*index as usize]);
        }

        // 2. Source-visible names → canonical declaration path.
        // Includes same-file declarations and imported source symbols (aliased or not).
        // When file_visibility is populated (production contexts), this is the
        // *only* path for cross-file name lookup. The fallback below is only for test
        // contexts that do not set file_visibility.
        // Skip receiver methods: they must be called via receiver syntax, and the
        // receiver method catalog handles their lookup.
        if let Some(file_visibility) = &self.file_visibility {
            if let Some(canonical_path) = file_visibility.visible_source_names.get(name)
                && let Some(declaration) = self
                    .shared
                    .lookups
                    .declaration_table
                    .get_by_path(canonical_path)
                && !declaration.value.is_receiver_function()
            {
                return Some(declaration);
            }
            // file_visibility is set but name not found — do not fall back.
            // This ensures import aliases hide the original name.
            return None;
        }

        // 3. Fallback for contexts that do not set file_visibility
        // (e.g. synthetic evaluation contexts and some unit-test helpers).
        self.shared
            .lookups
            .declaration_table
            .get_visible_non_receiver_by_name(*name, self.visible_declaration_ids.as_ref())
    }

    pub(crate) fn lookup_generic_function_template(
        &self,
        function_path: &InternedPath,
    ) -> Option<&GenericFunctionTemplate> {
        self.shared
            .lookups
            .generic_function_templates_by_path
            .get(function_path)
    }

    pub(crate) fn lookup_receiver_method(
        &self,
        receiver: &ReceiverKey,
        method_name: StringId,
    ) -> Option<&ReceiverMethodEntry> {
        // Production contexts use the header-built file visibility to determine
        // which receiver methods are visible.
        if let Some(file_visibility) = &self.file_visibility {
            let paths = file_visibility.visible_receiver_methods.get(&method_name)?;
            for visible_method in paths {
                let entry = self
                    .receiver_methods
                    .by_function_path
                    .get(&visible_method.function_path)?;
                if &entry.receiver == receiver {
                    return Some(entry);
                }
            }
            return None;
        }

        // Test contexts that don't set file_visibility fall back to the old
        // source-file + exported check.
        let entry = self
            .receiver_methods
            .by_receiver_and_name
            .get(&(receiver.to_owned(), method_name))?;

        let current_source_file = self.source_file_scope.as_ref()?;
        if &entry.source_file == current_source_file || entry.exported {
            Some(entry)
        } else {
            None
        }
    }

    pub(crate) fn lookup_visible_receiver_method_by_name(
        &self,
        method_name: StringId,
    ) -> Option<&ReceiverMethodEntry> {
        // Production contexts use the header-built file visibility to determine
        // which receiver methods are visible.
        if let Some(file_visibility) = &self.file_visibility {
            let paths = file_visibility.visible_receiver_methods.get(&method_name)?;
            for visible_method in paths {
                if let Some(entry) = self
                    .receiver_methods
                    .by_function_path
                    .get(&visible_method.function_path)
                {
                    return Some(entry);
                }
            }
            return None;
        }

        // Test contexts that don't set file_visibility fall back to the old
        // source-file + exported check.
        let current_source_file = self.source_file_scope.as_ref()?;
        let entries = self.receiver_methods.by_method_name.get(&method_name)?;

        entries
            .iter()
            .find(|entry| &entry.source_file == current_source_file)
            .or_else(|| entries.iter().find(|entry| entry.exported))
    }

    /// Look up a visible external function by its source-level name.
    pub(crate) fn lookup_visible_external_function(
        &self,
        name: StringId,
    ) -> Option<(ExternalFunctionId, &ExternalFunctionDef)> {
        let file_visibility = self.file_visibility.as_ref()?;
        let symbol_id = *file_visibility.visible_external_symbols.get(&name)?;
        let ExternalSymbolId::Function(function_id) = symbol_id else {
            return None;
        };
        let definition = self
            .external_package_registry
            .get_function_by_id(function_id)?;
        if definition.receiver_type.is_some() {
            return None;
        }

        Some((function_id, definition))
    }

    /// Look up a visible external type by its source-level name.
    pub(crate) fn lookup_visible_external_type(
        &self,
        name: StringId,
    ) -> Option<(ExternalTypeId, &ExternalTypeDef)> {
        let file_visibility = self.file_visibility.as_ref()?;
        let symbol_id = *file_visibility.visible_external_symbols.get(&name)?;
        let ExternalSymbolId::Type(type_id) = symbol_id else {
            return None;
        };
        self.external_package_registry
            .get_type_by_id(type_id)
            .map(|definition| (type_id, definition))
    }

    /// Look up a visible external constant by its source-level name.
    pub(crate) fn lookup_visible_external_constant(
        &self,
        name: StringId,
    ) -> Option<(ExternalConstantId, &ExternalConstantDef)> {
        let file_visibility = self.file_visibility.as_ref()?;
        let symbol_id = *file_visibility.visible_external_symbols.get(&name)?;
        let ExternalSymbolId::Constant(constant_id) = symbol_id else {
            return None;
        };
        self.external_package_registry
            .get_constant_by_id(constant_id)
            .map(|definition| (constant_id, definition))
    }

    /// Look up a visible external receiver method by receiver type and method name.
    ///
    /// WHAT: only considers external functions in
    ///       `file_visibility.visible_external_receiver_methods`; checks receiver compatibility
    ///       against the definition's `receiver_type`.
    /// WHY: package-scoped external symbols must respect file-local visibility.
    pub(crate) fn lookup_visible_external_method(
        &self,
        receiver_type_id: TypeId,
        method_name: StringId,
        type_environment: &TypeEnvironment,
    ) -> Option<(ExternalFunctionId, &ExternalFunctionDef)> {
        let file_visibility = self.file_visibility.as_ref()?;
        let visible_function_ids = file_visibility
            .visible_external_receiver_methods
            .get(&method_name)?;

        for function_id in visible_function_ids {
            let definition = self
                .external_package_registry
                .get_function_by_id(*function_id)?;
            let expected_signature = definition.receiver_type.as_ref()?;
            if external_signature_type_matches_type_id(
                expected_signature,
                receiver_type_id,
                type_environment,
            ) {
                return Some((*function_id, definition));
            }
        }

        None
    }

    /// Check whether a name is a visible type alias, regardless of whether its target
    /// has been resolved yet.
    ///
    /// WHAT: used by expression parsing to give a precise diagnostic when a type alias
    /// is mistakenly used in value position.
    pub(crate) fn is_visible_type_alias_name(&self, name: StringId) -> bool {
        self.shared
            .file_visibility
            .as_ref()
            .is_some_and(|file_visibility| {
                file_visibility.visible_type_alias_names.contains_key(&name)
            })
    }
}
