//! Dev-server HTML rendering for `CompilerDiagnostic`.
//!
//! WHAT: converts structured diagnostics into escaped HTML cards for the dev-server error page.
//! WHY: the dev-server needs clickable source links and readable diagnostic output.

use crate::compiler_frontend::compiler_messages::render::{
    DiagnosticRenderContext, diagnostic_type_name, display_column_number, display_line_number,
    resolve_source_file_path, special_file_name_from_path, type_mismatch_context_name,
};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticPayload, DiagnosticSeverity, NamingConvention,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use std::path::Path;

pub(crate) fn render_diagnostics_html(
    diagnostics: &[CompilerDiagnostic],
    project_root: &Path,
    string_table: &StringTable,
) -> String {
    let context = DiagnosticRenderContext::new(string_table);
    render_diagnostics_html_with_context(diagnostics, project_root, context)
}

pub(crate) fn render_diagnostics_html_with_context(
    diagnostics: &[CompilerDiagnostic],
    project_root: &Path,
    context: DiagnosticRenderContext<'_>,
) -> String {
    if diagnostics.is_empty() {
        return String::from("<p>No compiler diagnostics available.</p>");
    }

    diagnostics
        .iter()
        .map(|d| render_diagnostic_card(d, project_root, context))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_diagnostic_card(
    diagnostic: &CompilerDiagnostic,
    project_root: &Path,
    context: DiagnosticRenderContext<'_>,
) -> String {
    let string_table = context.string_table;
    let descriptor = diagnostic.kind.descriptor();
    let severity_badge = match diagnostic.severity {
        DiagnosticSeverity::Error => "ERROR",
        DiagnosticSeverity::Warning => "WARNING",
        DiagnosticSeverity::Note => "NOTE",
    };
    let badge_class = match diagnostic.severity {
        DiagnosticSeverity::Error => "badge",
        DiagnosticSeverity::Warning => "badge warning",
        DiagnosticSeverity::Note => "badge info",
    };

    let mut details = String::from("<ul class=\"detail-list\">");

    let resolved_path = resolve_source_file_path(&diagnostic.primary_location.scope, string_table);
    let display_root = match std::fs::canonicalize(project_root) {
        Ok(canonical_root) => canonical_root,
        Err(_) => project_root.to_path_buf(),
    };
    let display_label =
        crate::compiler_frontend::compiler_messages::render::relative_display_path_from_root(
            &resolved_path,
            &display_root,
        );
    let line = display_line_number(diagnostic.primary_location.start_pos.line_number);
    let column = display_column_number(diagnostic.primary_location.start_pos.char_column);

    details.push_str(&format!(
        "<li><a href=\"file://{}\">{}</a> — line {}, col {}</li>",
        escape_html(&resolved_path.to_string_lossy()),
        escape_html(&display_label),
        line,
        column
    ));

    // Payload message
    let message = payload_message(&diagnostic.payload, context);
    if !message.is_empty() {
        details.push_str(&format!("<li>{}</li>", escape_html(&message)));
    }

    details.push_str("</ul>");

    format!(
        "<article class=\"diagnostic\">\
         <header><span class=\"{badge_class}\">{severity_badge}</span> \
         <code>{code}</code> {title}</header>\
         {details}\
         </article>",
        code = escape_html(descriptor.code),
        title = escape_html(descriptor.title),
    )
}

fn payload_message(payload: &DiagnosticPayload, context: DiagnosticRenderContext<'_>) -> String {
    let string_table = context.string_table;
    match payload {
        DiagnosticPayload::InfrastructureError { msg, .. } => msg.clone(),
        DiagnosticPayload::UnusedName { name } => {
            format!("Unused name '{}'", string_table.resolve(*name))
        }
        DiagnosticPayload::UnreachableMatchArm => "This match arm is unreachable".into(),
        DiagnosticPayload::BstFilePathInTemplateOutput { path } => {
            format!(
                "Beanstalk source path '{}' is being inserted into template output",
                string_table.resolve(*path)
            )
        }
        DiagnosticPayload::LargeTrackedAsset { path, byte_size } => {
            let mib = *byte_size as f64 / (1024.0 * 1024.0);
            format!(
                "Large tracked asset '{}' ({mib:.1} MiB)",
                string_table.resolve(*path)
            )
        }
        DiagnosticPayload::IdentifierNamingConvention {
            name,
            expected_style,
        } => {
            let style_name = match expected_style {
                NamingConvention::CamelCase => "CamelCase",
                NamingConvention::LowercaseWithUnderscores => "lowercase_with_underscores",
                NamingConvention::UppercaseWithUnderscores => "UPPER_CASE_WITH_UNDERSCORES",
                NamingConvention::LowercaseOrUppercaseWithUnderscores => {
                    "lowercase_with_underscores or UPPER_CASE_WITH_UNDERSCORES"
                }
            };
            format!(
                "Identifier '{}' should use {}",
                string_table.resolve(*name),
                style_name
            )
        }
        DiagnosticPayload::ImportAliasCaseMismatch { alias, symbol } => {
            format!(
                "Import alias '{}' uses different leading-name case than imported symbol '{}'",
                string_table.resolve(*alias),
                string_table.resolve(*symbol)
            )
        }
        DiagnosticPayload::MalformedTemplate { message } => {
            format!("Malformed template: {}", string_table.resolve(*message))
        }
        DiagnosticPayload::OldPrefixDeclarationSyntax => {
            "`#` is no longer a declaration prefix. Use `name #= value` for inferred compile-time constants or `name #Type = value` for explicit constant types. Module visibility is controlled by file/module boundaries and `#mod.bst` facades.".into()
        }
        DiagnosticPayload::TypeMismatch {
            expected,
            found,
            context: mismatch_context,
        } => {
            format!(
                "Type mismatch in {} context. Expected {}, found {}.",
                type_mismatch_context_name(*mismatch_context),
                diagnostic_type_name(*expected, context),
                diagnostic_type_name(*found, context)
            )
        }
        DiagnosticPayload::MissingImportTarget { .. }
        | DiagnosticPayload::AmbiguousImportTarget { .. }
        | DiagnosticPayload::BareFileImport { .. }
        | DiagnosticPayload::DirectSpecialFileImport { .. }
        | DiagnosticPayload::ImportNameCollision { .. }
        | DiagnosticPayload::NotExportedBySourceFile { .. }
        | DiagnosticPayload::NotExportedByFacade { .. }
        | DiagnosticPayload::MissingModuleFacade { .. }
        | DiagnosticPayload::MissingPackageSymbol { .. }
        | DiagnosticPayload::CrossModuleImportNotExported { .. }
        | DiagnosticPayload::InvalidImportPath { .. }
        | DiagnosticPayload::DirectSymbolPathImport { .. }
        | DiagnosticPayload::InvalidNamespaceDefaultName { .. }
        | DiagnosticPayload::DuplicateImportSurfaceMember { .. }
        | DiagnosticPayload::ExplicitBstExtension { .. }
        | DiagnosticPayload::UnsupportedExternalExtension { .. }
        | DiagnosticPayload::InvalidExternalLibrary { .. }
        | DiagnosticPayload::ReceiverMethodImportRequiresVisibleReceiverType { .. } => {
            import_payload_message(payload, string_table)
        }
        DiagnosticPayload::DeferredFeature { reason } => {
            super::deferred_feature_message(reason, string_table)
        }
        DiagnosticPayload::UnsupportedExternalFunction {
            function_name,
            package_path,
            backend_name,
        } => super::unsupported_external_function_message(
            *function_name,
            *package_path,
            *backend_name,
            string_table,
        ),
        DiagnosticPayload::NamespaceMisuse {
            name,
            expected,
            found,
        } => super::namespace_misuse_message(*name, *expected, *found, string_table),
        DiagnosticPayload::ImportRecordUsedAsValue { record_name } => {
            super::import_record_used_as_value_message(*record_name, string_table)
        }
        DiagnosticPayload::ConstRecordUsedAsValue { record_name } => {
            super::const_record_used_as_value_message(*record_name, string_table)
        }
        DiagnosticPayload::NestedTraversal { record_name } => {
            super::nested_traversal_message(*record_name, string_table)
        }
        DiagnosticPayload::NamespaceTypeValueMisuse {
            name,
            expected,
            found,
        } => {
            super::namespace_type_value_misuse_message(*name, *expected, *found, string_table)
        }
        DiagnosticPayload::ShadowedName { name, .. } => {
            format!(
                "Name '{}' is already declared in this scope. Shadowing is not supported.",
                string_table.resolve(*name)
            )
        }
        DiagnosticPayload::ReservedNameCollision { name, reserved_by } => {
            let owner = match reserved_by {
                crate::compiler_frontend::compiler_messages::ReservedNameOwner::BuiltinType => {
                    "builtin language type"
                }
                crate::compiler_frontend::compiler_messages::ReservedNameOwner::Keyword => {
                    "reserved language keyword"
                }
            };
            format!(
                "'{}' collides with a reserved {}.",
                string_table.resolve(*name),
                owner
            )
        }
        DiagnosticPayload::InvalidThisUsage { reason } => {
            super::invalid_this_usage_message(*reason, string_table)
        }
        DiagnosticPayload::InvalidReceiverDeclaration { reason } => {
            super::invalid_receiver_declaration_message(*reason, string_table)
        }
        DiagnosticPayload::InvalidControlFlowStatement { reason } => {
            super::invalid_control_flow_statement_message(*reason)
        }
        DiagnosticPayload::InvalidDeclaration { reason, name } => {
            super::invalid_declaration_message(reason.clone(), *name, string_table)
        }
        DiagnosticPayload::InvalidAssignmentTarget {
            reason,
            target_name,
            target_type,
        } => super::invalid_assignment_target_message(*reason, *target_name, *target_type, context),
        DiagnosticPayload::InvalidMultiBind {
            reason,
            target_name,
        } => super::invalid_multi_bind_message(*reason, *target_name, string_table),
        DiagnosticPayload::InvalidBuiltinCall {
            reason,
            builtin_name,
        } => super::invalid_builtin_call_message(*reason, *builtin_name, string_table),
        DiagnosticPayload::InvalidReceiverCall {
            reason,
            receiver_type,
            method_name,
        } => super::invalid_receiver_call_message(
            *reason,
            *receiver_type,
            *method_name,
            string_table,
        ),
        DiagnosticPayload::InvalidCopyTarget { reason } => {
            super::invalid_copy_target_message(*reason)
        }
        DiagnosticPayload::InvalidFieldAccess {
            reason,
            field_name,
            receiver_type,
        } => super::invalid_field_access_message(*reason, *field_name, *receiver_type, context),
        DiagnosticPayload::InvalidMatchPattern {
            reason,
            variant_name,
            scrutinee_name: _,
        } => super::invalid_match_pattern_message(*reason, *variant_name, string_table),
        DiagnosticPayload::NonExhaustiveMatch {
            reason,
            missing_variants,
            ..
        } => super::non_exhaustive_match_message(*reason, missing_variants, string_table),
        DiagnosticPayload::InvalidResultHandling { reason } => reason.message().to_string(),
        DiagnosticPayload::InvalidTemplateSlot { reason, slot_name } => {
            super::invalid_template_slot_message(*reason, *slot_name, string_table)
        }
        DiagnosticPayload::CompileTimeEvaluationError { reason, operation } => {
            super::compile_time_evaluation_error_message(*reason, *operation, string_table)
        }
        DiagnosticPayload::EmptyCollectionTypeAmbiguity => {
            "Cannot infer the element type of an empty collection literal".to_string()
        }
        DiagnosticPayload::UnsupportedOperatorTypes { category, lhs, rhs } => {
            super::unsupported_operator_types_message(*category, *lhs, *rhs, context)
        }
        DiagnosticPayload::InvalidResultOperand {
            reason,
            category,
            operand_type,
        } => super::invalid_result_operand_message(*reason, *category, *operand_type, context),
        DiagnosticPayload::IncompatibleChoiceComparison { reason, lhs, rhs } => {
            super::incompatible_choice_comparison_message(reason, *lhs, *rhs, context)
        }
        DiagnosticPayload::MultipleMutableBorrows { .. }
        | DiagnosticPayload::SharedMutableConflict { .. }
        | DiagnosticPayload::UseAfterPossibleMove { .. }
        | DiagnosticPayload::MoveWhileBorrowed { .. }
        | DiagnosticPayload::WholeObjectBorrowConflict { .. }
        | DiagnosticPayload::InvalidMutableAccess { .. }
        | DiagnosticPayload::InvalidAccessAfterPossibleOwnershipTransfer { .. }
        | DiagnosticPayload::UseOfUninitializedLocal { .. } => {
            borrow_payload_message(payload, string_table)
        }
        DiagnosticPayload::InvalidConfig { key, reason } => {
            super::invalid_config_message(*key, reason, string_table)
        }
        DiagnosticPayload::InvalidCallShape { reason, .. } => {
            super::invalid_call_shape_message(reason.clone())
        }
        DiagnosticPayload::InvalidReturnShape { reason } => {
            super::invalid_return_shape_message(*reason)
        }
        DiagnosticPayload::InvalidGenericInstantiation { type_name, reason } => {
            super::invalid_generic_instantiation_message(*type_name, reason, string_table)
        }
        DiagnosticPayload::InvalidRangeOperand {
            operand,
            found_type,
        } => super::invalid_range_operand_message(*operand, *found_type, context),
        DiagnosticPayload::UnsupportedBuilderPackage { package_path } => {
            super::unsupported_builder_package_message(*package_path, string_table)
        }
        DiagnosticPayload::InvalidPageMetadata { key, reason } => {
            super::invalid_page_metadata_message(*key, *reason, string_table)
        }
        DiagnosticPayload::InvalidCompileTimePath { path, reason } => {
            super::invalid_compile_time_path_message(path, *reason, string_table)
        }
        DiagnosticPayload::InvalidExpression => super::invalid_expression_message(),
        DiagnosticPayload::CommonSyntaxMistake { reason } => {
            super::common_syntax_mistake_message(reason, string_table)
        }
        DiagnosticPayload::MissingOperatorOperand { operator, position } => {
            super::missing_operator_operand_message(*operator, *position, string_table)
        }
        DiagnosticPayload::InvalidStandaloneStatement { reason } => {
            super::invalid_standalone_statement_message(*reason)
        }
        DiagnosticPayload::ExpectedSymbolStatement => super::expected_symbol_statement_message(),
        DiagnosticPayload::MissingCollectionItem => super::missing_collection_item_message(),
        DiagnosticPayload::InvalidMatchArm { reason } => super::invalid_match_arm_message(*reason),
        DiagnosticPayload::InvalidLoopHeader { reason } => {
            super::invalid_loop_header_message(*reason, context)
        }
        DiagnosticPayload::InvalidStatementPosition { reason } => {
            super::invalid_statement_position_message(*reason)
        }
        _ => String::new(),
    }
}

fn import_payload_message(payload: &DiagnosticPayload, string_table: &StringTable) -> String {
    match payload {
        DiagnosticPayload::MissingImportTarget { path } => {
            format!(
                "Cannot resolve import '{}'.",
                path.to_portable_string(string_table)
            )
        }
        DiagnosticPayload::AmbiguousImportTarget { path } => {
            format!(
                "Ambiguous import target '{}'. Use a more specific path.",
                path.to_portable_string(string_table)
            )
        }
        DiagnosticPayload::BareFileImport { path } => {
            format!(
                "Bare file imports are not supported; import an exported symbol from the file '{}'.",
                path.to_portable_string(string_table)
            )
        }
        DiagnosticPayload::DirectSpecialFileImport { path } => {
            let special_file = special_file_name_from_path(path, string_table);
            format!(
                "Cannot import directly from '{special_file}' via '{}'. Import exported symbols through the module path instead.",
                path.to_portable_string(string_table)
            )
        }
        DiagnosticPayload::ImportNameCollision { name, .. } => {
            format!(
                "Import name collision: '{}' is already visible in this file.",
                string_table.resolve(*name)
            )
        }
        DiagnosticPayload::NotExportedBySourceFile { symbol_path } => {
            format!(
                "Cannot import '{}' because it is not exported.",
                symbol_path.to_portable_string(string_table)
            )
        }
        DiagnosticPayload::NotExportedByFacade {
            requested_path,
            facade_name,
            facade_type,
        } => {
            let path_str = requested_path.to_portable_string(string_table);
            let facade_name_str = string_table.resolve(*facade_name);
            match facade_type {
                crate::compiler_frontend::compiler_messages::ImportFacadeType::SourceLibrary => {
                    format!(
                        "Cannot import '{}' from source library '@{}' because it is not exported by the library facade.",
                        path_str, facade_name_str
                    )
                }
                crate::compiler_frontend::compiler_messages::ImportFacadeType::ModuleRoot => {
                    format!(
                        "Cannot import '{}' from module '{}' because it is not exported by the module's facade.",
                        path_str, facade_name_str
                    )
                }
            }
        }
        DiagnosticPayload::MissingModuleFacade { symbol_path } => {
            format!(
                "Cannot import '{}' because the target module has no #mod.bst facade. Import a concrete file from inside the same module, or add #mod.bst to define the module's public import surface.",
                symbol_path.to_portable_string(string_table)
            )
        }
        DiagnosticPayload::MissingPackageSymbol {
            symbol,
            package_path,
        } => {
            format!(
                "Cannot import '{}' from package '{}': symbol not found.",
                string_table.resolve(*symbol),
                string_table.resolve(*package_path)
            )
        }
        DiagnosticPayload::CrossModuleImportNotExported { symbol_path } => {
            format!(
                "Cannot import '{}' because it is not exported by the target module's facade.",
                symbol_path.to_portable_string(string_table)
            )
        }
        DiagnosticPayload::InvalidImportPath { path, reason } => {
            super::invalid_import_path_message(path, *reason, string_table)
        }
        DiagnosticPayload::DirectSymbolPathImport { path } => {
            super::direct_symbol_path_import_message(path, string_table)
        }
        DiagnosticPayload::InvalidNamespaceDefaultName { path } => {
            super::invalid_namespace_default_name_message(path, string_table)
        }
        DiagnosticPayload::DuplicateImportSurfaceMember {
            surface_path,
            member_name,
        } => {
            super::duplicate_import_surface_member_message(surface_path, *member_name, string_table)
        }
        DiagnosticPayload::ExplicitBstExtension { path } => {
            super::explicit_bst_extension_message(path, string_table)
        }
        DiagnosticPayload::UnsupportedExternalExtension { path, extension } => {
            super::unsupported_external_extension_message(path, *extension, string_table)
        }
        DiagnosticPayload::InvalidExternalLibrary { path, message } => {
            super::invalid_external_library_message(path, *message, string_table)
        }
        DiagnosticPayload::ReceiverMethodImportRequiresVisibleReceiverType {
            method_name,
            receiver_type_name,
        } => super::receiver_method_import_requires_visible_receiver_type_message(
            *method_name,
            *receiver_type_name,
            string_table,
        ),
        _ => String::new(),
    }
}

fn borrow_payload_message(payload: &DiagnosticPayload, string_table: &StringTable) -> String {
    match payload {
        DiagnosticPayload::MultipleMutableBorrows { place, .. } => {
            super::multiple_mutable_borrows_message(place, string_table)
        }
        DiagnosticPayload::SharedMutableConflict {
            place,
            existing_access,
            requested_access,
            conflicting_place,
            ..
        } => super::shared_mutable_conflict_message(
            place,
            *existing_access,
            *requested_access,
            conflicting_place.as_ref(),
            string_table,
        ),
        DiagnosticPayload::UseAfterPossibleMove { place, .. } => {
            super::use_after_possible_move_message(place, string_table)
        }
        DiagnosticPayload::MoveWhileBorrowed {
            place,
            existing_access,
            ..
        } => super::move_while_borrowed_message(place, *existing_access, string_table),
        DiagnosticPayload::WholeObjectBorrowConflict {
            whole_place,
            part_place,
            ..
        } => super::whole_object_borrow_conflict_message(whole_place, part_place, string_table),
        DiagnosticPayload::InvalidMutableAccess {
            place,
            reason,
            conflicting_place,
        } => super::invalid_mutable_access_message(
            place,
            *reason,
            conflicting_place.as_ref(),
            string_table,
        ),
        DiagnosticPayload::InvalidAccessAfterPossibleOwnershipTransfer { place } => {
            super::invalid_access_after_possible_ownership_transfer_message(place, string_table)
        }
        DiagnosticPayload::UseOfUninitializedLocal { place } => {
            super::use_of_uninitialized_local_message(place, string_table)
        }
        _ => String::new(),
    }
}

fn escape_html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}
