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
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::generic_functions::GenericFunctionTemplate;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;

/// Resolved struct constructor metadata for identifier-led expression dispatch.
///
/// WHAT: carries canonical nominal identity plus the original AST field declarations used for
/// constructor defaults.
/// WHY: constructor routing should be driven by semantic TypeId/type-environment facts, while
/// default expressions still live on AST declarations.
pub(crate) struct SourceStructConstructor<'a> {
    pub(crate) struct_path: InternedPath,
    pub(crate) fields: &'a [Declaration],
    pub(crate) struct_value_mode: &'a ValueMode,
    pub(crate) type_id: TypeId,
}

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

    /// Return whether a declaration is visible as an authored `#` constant in this context.
    ///
    /// WHAT: accepts body-local constants recorded during scope growth plus top-level
    /// module constants from either seeded header contexts or completed module lookups.
    /// WHY: fixed-capacity type syntax must reject foldable runtime bindings while still
    /// allowing visible explicit constants before and after the final lookup package exists.
    pub(crate) fn is_explicit_compile_time_constant(&self, declaration: &Declaration) -> bool {
        self.explicit_compile_time_constant_declarations
            .contains(&declaration.id)
            || self
                .shared
                .lookups
                .module_constants
                .iter()
                .any(|constant| constant.id == declaration.id)
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

    /// Return the semantic role of a declaration without inspecting display-only type spelling.
    pub(crate) fn semantic_kind_for_declaration(
        &self,
        declaration: &Declaration,
        type_environment: &TypeEnvironment,
    ) -> DeclarationSemanticKind {
        if let Some(kind) = self
            .lookups
            .declaration_semantics
            .kind_for_path(&declaration.id)
        {
            return kind;
        }

        match &declaration.value.kind {
            ExpressionKind::Function(..) => DeclarationSemanticKind::Function,
            ExpressionKind::NoValue => match type_environment.get(declaration.value.type_id) {
                Some(TypeDefinition::Struct(..)) => DeclarationSemanticKind::Struct,
                Some(TypeDefinition::Choice(..)) => DeclarationSemanticKind::Choice,
                _ => DeclarationSemanticKind::Value,
            },
            _ if declaration.value.is_compile_time_constant() => DeclarationSemanticKind::Constant,
            _ => DeclarationSemanticKind::Value,
        }
    }

    /// Resolve callable metadata for a source declaration.
    ///
    /// Top-level functions are classified from the environment's resolved signature table.
    /// Body-local functions carry their signature directly in `ExpressionKind::Function`.
    pub(crate) fn source_callable_signature<'a>(
        &'a self,
        declaration: &'a Declaration,
    ) -> Option<&'a FunctionSignature> {
        if let Some(resolved_signature) = self
            .lookups
            .resolved_function_signatures_by_path
            .get(&declaration.id)
        {
            return Some(&resolved_signature.signature);
        }

        match &declaration.value.kind {
            ExpressionKind::Function(signature, _) => Some(signature),
            _ => None,
        }
    }

    /// Resolve constructor metadata for a source struct declaration.
    pub(crate) fn source_struct_constructor<'a>(
        &'a self,
        declaration: &'a Declaration,
        type_environment: &TypeEnvironment,
    ) -> Option<SourceStructConstructor<'a>> {
        if self.semantic_kind_for_declaration(declaration, type_environment)
            != DeclarationSemanticKind::Struct
        {
            return None;
        }

        let type_id = declaration.value.type_id;
        let struct_path = type_environment.nominal_path(type_id)?.to_owned();
        let fields = self
            .resolved_struct_fields_by_path
            .as_ref()
            .and_then(|map| map.get(&struct_path))
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        Some(SourceStructConstructor {
            struct_path,
            fields,
            struct_value_mode: &declaration.value.value_mode,
            type_id,
        })
    }

    /// Return whether a source declaration is a choice type.
    pub(crate) fn is_source_choice_declaration(
        &self,
        declaration: &Declaration,
        type_environment: &TypeEnvironment,
    ) -> bool {
        self.semantic_kind_for_declaration(declaration, type_environment)
            == DeclarationSemanticKind::Choice
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

        // Unit-test contexts can omit file visibility. Production calls always use
        // header-built visibility, so the fallback keeps tests deterministic without
        // reintroducing a separate receiver-method export flag.
        let entries = self
            .receiver_methods
            .by_receiver_and_name
            .get(&(receiver.to_owned(), method_name))?;

        let current_source_file = self.source_file_scope.as_ref()?;
        entries
            .iter()
            .find(|entry| &entry.source_file == current_source_file)
            .or_else(|| entries.first())
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

        // Unit-test contexts can omit file visibility. Production calls always use
        // header-built visibility, so the fallback keeps tests deterministic without
        // reintroducing a separate receiver-method export flag.
        let current_source_file = self.source_file_scope.as_ref()?;
        let entries = self.receiver_methods.by_method_name.get(&method_name)?;

        entries
            .iter()
            .find(|entry| &entry.source_file == current_source_file)
            .or_else(|| entries.first())
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

        Some((function_id, definition))
    }

    /// Look up the source location that made an external symbol visible in this file.
    ///
    /// WHAT: returns the import-site location for an explicit external import, or
    /// `SourceLocation::default()` for prelude-injected symbols that have no authored source.
    /// WHY: AST duplicate-declaration diagnostics need a meaningful secondary label
    /// pointing to the import that made the symbol visible.
    pub(crate) fn lookup_visible_external_function_location(
        &self,
        name: StringId,
    ) -> Option<SourceLocation> {
        let file_visibility = self.file_visibility.as_ref()?;
        Some(
            file_visibility
                .visible_external_symbol_locations
                .get(&name)
                .cloned()
                .unwrap_or_default(),
        )
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

    /// Check whether a visible source binding points at a nominal type declaration.
    ///
    /// WHAT: uses canonical declaration paths instead of source spelling conventions.
    /// WHY: values may violate naming conventions and receive warnings, but namespace diagnostics
    /// must rely on the header/AST type metadata that identifies real type declarations.
    pub(crate) fn is_nominal_type_declaration_path(&self, path: &InternedPath) -> bool {
        if self.nominal_type_ids_by_path.contains_key(path)
            || self
                .lookups
                .module_symbols
                .nominal_type_paths
                .contains(path)
        {
            return true;
        }

        self.lookups
            .declaration_semantics
            .kind_for_path(path)
            .is_some_and(|kind| {
                matches!(
                    kind,
                    DeclarationSemanticKind::Struct | DeclarationSemanticKind::Choice
                )
            })
    }
}
