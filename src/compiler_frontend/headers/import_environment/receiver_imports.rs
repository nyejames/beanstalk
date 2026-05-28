//! Receiver-method visibility and pending validation.
//!
//! WHAT: adds receiver methods to the file-local receiver catalog and validates that explicit
//! grouped receiver-method imports have a visible receiver type.
//! WHY: receiver methods live in a separate call namespace from ordinary value bindings, so
//! their visibility rules need dedicated helpers.
//! MUST NOT: register ordinary value/type imports or parse executable bodies.

use super::{
    FileVisibility, ImportEnvironmentBuilder, PendingReceiverMethodValidation,
    ReceiverMethodVisibility,
};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::external_packages::{
    ExternalAbiType, ExternalFunctionId, ExternalSignatureType, ExternalSymbolId,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

impl<'a> ImportEnvironmentBuilder<'a> {
    pub(super) fn add_visible_receiver_method(
        file_visibility: &mut FileVisibility,
        local_name: StringId,
        function_path: &InternedPath,
        location: SourceLocation,
    ) {
        let methods = file_visibility
            .visible_receiver_methods
            .entry(local_name)
            .or_default();

        if methods
            .iter()
            .any(|method| method.function_path == *function_path)
        {
            return;
        }

        methods.push(ReceiverMethodVisibility {
            function_path: function_path.clone(),
            location,
        });
    }

    pub(super) fn add_visible_external_receiver_method(
        file_visibility: &mut FileVisibility,
        local_name: StringId,
        function_id: ExternalFunctionId,
    ) {
        let methods = file_visibility
            .visible_external_receiver_methods
            .entry(local_name)
            .or_default();

        if methods.contains(&function_id) {
            return;
        }

        methods.push(function_id);
    }

    /// Validate all pending explicit grouped receiver-method imports for this file.
    // The typed diagnostic payload is still large enough to trigger clippy::result_large_err here.
    #[allow(clippy::result_large_err)]
    pub(super) fn validate_pending_receiver_methods(
        &mut self,
        file_visibility: &FileVisibility,
    ) -> Result<(), CompilerDiagnostic> {
        let pending_validations = self.pending_receiver_validations.clone();
        for pending in &pending_validations {
            let is_visible = if let Some(source_path) = &pending.source_path {
                self.source_receiver_type_is_visible(file_visibility, source_path)
            } else if let Some(function_id) = pending.external_function_id {
                self.external_package_registry
                    .get_function_by_id(function_id)
                    .and_then(|function| function.receiver_type.as_ref())
                    .is_some_and(|receiver_type| {
                        self.external_receiver_type_is_visible(file_visibility, receiver_type)
                    })
            } else {
                false
            };

            if !is_visible {
                let receiver_type_name = self.receiver_type_name_for_pending(pending);
                return Err(
                    CompilerDiagnostic::receiver_method_import_requires_visible_receiver_type(
                        pending.local_name,
                        receiver_type_name,
                        pending.location.clone(),
                    ),
                );
            }
        }
        Ok(())
    }

    /// Whether a source receiver type is visible in the importing file.
    ///
    /// WHY: explicit grouped receiver-method imports require the receiver type to be visible.
    /// Builtin scalar receiver types are always language-visible.
    fn source_receiver_type_is_visible(
        &self,
        file_visibility: &FileVisibility,
        receiver_method_path: &InternedPath,
    ) -> bool {
        let Some(receiver_name) = self
            .module_symbols
            .receiver_method_receiver_names
            .get(receiver_method_path)
        else {
            return false;
        };

        // Builtin scalar receiver types are language-visible.
        let receiver_name_str = self.string_table.resolve(*receiver_name);
        if matches!(
            receiver_name_str,
            "Int" | "Float" | "Bool" | "String" | "Char"
        ) {
            return true;
        }

        let Some(receiver_type_path) =
            self.source_receiver_nominal_type_path(receiver_method_path, *receiver_name)
        else {
            // Header import preparation does not resolve type-alias targets. When a
            // visible alias exists, AST keeps the exact transparent-alias check against
            // resolved types so legitimate facade aliases are not rejected too early.
            return !file_visibility.visible_type_alias_names.is_empty();
        };

        // Directly imported nominal type. This is path-based so aliases such as
        // `import @counter { Counter as C }` still satisfy receiver visibility.
        if file_visibility
            .visible_declaration_paths
            .contains(&receiver_type_path)
        {
            return true;
        }

        // Type member exposed by a namespace import from the same surface.
        if file_visibility
            .visible_namespace_records
            .values()
            .any(|record| {
                record.type_members.values().any(|member| {
                    matches!(
                        member,
                        super::NamespaceTypeMember::SourceDeclaration(type_path)
                        if type_path == &receiver_type_path
                    )
                })
            })
        {
            return true;
        }

        // Defer exact transparent-alias validation to AST, where alias targets are resolved.
        !file_visibility.visible_type_alias_names.is_empty()
    }

    /// Find the canonical nominal source type named by a source receiver method.
    ///
    /// WHY: imports may alias the receiver type locally, so visibility must be checked against
    /// the canonical type path rather than the local spelling in `visible_source_names`.
    fn source_receiver_nominal_type_path(
        &self,
        receiver_method_path: &InternedPath,
        receiver_name: StringId,
    ) -> Option<InternedPath> {
        let method_source = self
            .module_symbols
            .canonical_source_by_symbol_path
            .get(receiver_method_path)?;

        self.module_symbols
            .nominal_type_paths
            .iter()
            .find(|type_path| {
                type_path.name() == Some(receiver_name)
                    && self
                        .module_symbols
                        .canonical_source_by_symbol_path
                        .get(*type_path)
                        .is_some_and(|type_source| type_source == method_source)
            })
            .cloned()
    }

    /// Whether an external receiver type is visible in the importing file.
    ///
    /// WHY: explicit grouped external receiver-method imports require the receiver type to be
    ///      visible. `ExternalSignatureType::External(type_id)` is visible when that exact type
    ///      is imported or present in a namespace record. Builtin scalar ABI types are
    ///      language-visible. `Abi(Handle)` is not enough information.
    fn external_receiver_type_is_visible(
        &self,
        file_visibility: &FileVisibility,
        receiver_type: &ExternalSignatureType,
    ) -> bool {
        match receiver_type {
            ExternalSignatureType::External(type_id) => {
                // Directly imported external type.
                if file_visibility.visible_external_symbols.values().any(
                    |symbol_id| matches!(symbol_id, ExternalSymbolId::Type(tid) if tid == type_id),
                ) {
                    return true;
                }

                // Present in a namespace record.
                for record in file_visibility.visible_namespace_records.values() {
                    if record.type_members.values().any(|member| {
                        matches!(
                            member,
                            super::NamespaceTypeMember::ExternalSymbol(ExternalSymbolId::Type(tid))
                            if tid == type_id
                        )
                    }) {
                        return true;
                    }
                }

                false
            }
            ExternalSignatureType::Abi(abi_type) => {
                // Builtin scalar ABI receiver types are language-visible.
                matches!(
                    abi_type,
                    ExternalAbiType::I32
                        | ExternalAbiType::F64
                        | ExternalAbiType::Bool
                        | ExternalAbiType::Utf8Str
                        | ExternalAbiType::Char
                )
            }
            ExternalSignatureType::BuiltinError => false,
        }
    }

    /// Look up the receiver type name for a pending validation so diagnostics are specific.
    fn receiver_type_name_for_pending(
        &mut self,
        pending: &PendingReceiverMethodValidation,
    ) -> Option<StringId> {
        if let Some(source_path) = &pending.source_path {
            self.module_symbols
                .receiver_method_receiver_names
                .get(source_path)
                .copied()
        } else if let Some(function_id) = pending.external_function_id {
            self.external_package_registry
                .get_function_by_id(function_id)
                .and_then(|function| function.receiver_type.as_ref())
                .and_then(|receiver_type| match receiver_type {
                    ExternalSignatureType::External(type_id) => self
                        .external_package_registry
                        .get_type_by_id(*type_id)
                        .map(|type_def| self.string_table.intern(&type_def.name)),
                    ExternalSignatureType::Abi(abi_type) => match abi_type {
                        ExternalAbiType::I32 => Some(self.string_table.intern("Int")),
                        ExternalAbiType::F64 => Some(self.string_table.intern("Float")),
                        ExternalAbiType::Bool => Some(self.string_table.intern("Bool")),
                        ExternalAbiType::Utf8Str => Some(self.string_table.intern("String")),
                        ExternalAbiType::Char => Some(self.string_table.intern("Char")),
                        _ => None,
                    },
                    ExternalSignatureType::BuiltinError => None,
                })
        } else {
            None
        }
    }
}
