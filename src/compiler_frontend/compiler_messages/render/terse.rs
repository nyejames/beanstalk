//! Terse rendering for `CompilerDiagnostic`.
//!
//! WHAT: produces single-line machine-friendly diagnostic records.
//! WHY: CI, test runners, and IDEs often prefer compact output without ASCII art or colours.

use crate::compiler_frontend::compiler_messages::render::{
    DiagnosticRenderContext, diagnostic_type_name, display_column_number, display_line_number,
    relative_display_path_from_root, resolve_source_file_path, type_mismatch_context_name,
};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticPayload, DiagnosticSeverity,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;

pub(crate) fn format_terse_diagnostics(
    diagnostics: &[CompilerDiagnostic],
    string_table: &StringTable,
) -> Vec<String> {
    let context = DiagnosticRenderContext::new(string_table);
    format_terse_diagnostics_with_context(diagnostics, context)
}

pub(crate) fn format_terse_diagnostics_with_context(
    diagnostics: &[CompilerDiagnostic],
    context: DiagnosticRenderContext<'_>,
) -> Vec<String> {
    diagnostics
        .iter()
        .map(|d| format_terse_diagnostic_with_context(d, context))
        .collect()
}

pub(crate) fn format_terse_diagnostic_with_context(
    diagnostic: &CompilerDiagnostic,
    context: DiagnosticRenderContext<'_>,
) -> String {
    let string_table = context.string_table;
    let descriptor = diagnostic.kind.descriptor();
    let severity_char = match diagnostic.severity {
        DiagnosticSeverity::Error => 'E',
        DiagnosticSeverity::Warning => 'W',
        DiagnosticSeverity::Note => 'N',
    };

    let display_path = relative_display_path_from_root(
        &resolve_source_file_path(&diagnostic.primary_location.scope, string_table),
        &std::env::current_dir().unwrap_or_default(),
    );
    let sanitized_path = sanitize_terse_field(&display_path);
    let line = display_line_number(diagnostic.primary_location.start_pos.line_number);
    let column = display_column_number(diagnostic.primary_location.start_pos.char_column);

    let message = payload_message(&diagnostic.payload, context);

    format!(
        "{severity_char}|{}|{sanitized_path}|{line}:{column}|{}",
        descriptor.code,
        sanitize_terse_field(&message)
    )
}

fn payload_message(payload: &DiagnosticPayload, context: DiagnosticRenderContext<'_>) -> String {
    let string_table = context.string_table;
    match payload {
        DiagnosticPayload::InfrastructureError { msg, .. } => msg.clone(),
        DiagnosticPayload::ExpectedToken { expected, found } => {
            super::expected_token_message(expected, found.as_ref(), string_table)
        }
        DiagnosticPayload::UnexpectedToken { found } => {
            super::unexpected_token_message(found, string_table)
        }
        DiagnosticPayload::UnexpectedTrailingComma => "Unexpected trailing comma".into(),
        DiagnosticPayload::UnknownName { name, namespace } => {
            super::unknown_name_message(*name, *namespace, string_table)
        }
        DiagnosticPayload::TypeMismatch {
            expected,
            found,
            context: mismatch_context,
        } => {
            format!(
                "Type mismatch in {}: expected {}, found {}",
                type_mismatch_context_name(*mismatch_context),
                diagnostic_type_name(*expected, context),
                diagnostic_type_name(*found, context)
            )
        }
        DiagnosticPayload::DuplicateDeclaration { name, .. } => {
            super::duplicate_declaration_message(*name, string_table)
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
        DiagnosticPayload::BorrowConflict { .. }
        | DiagnosticPayload::MultipleMutableBorrows { .. }
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
        DiagnosticPayload::UnusedName { name } => {
            format!("Unused name '{}'", string_table.resolve(*name))
        }
        DiagnosticPayload::UnreachableMatchArm => "Unreachable match arm".into(),
        DiagnosticPayload::BstFilePathInTemplateOutput { .. } => {
            "Beanstalk source path in template output".into()
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
                crate::compiler_frontend::compiler_messages::NamingConvention::CamelCase => {
                    "CamelCase"
                }
                crate::compiler_frontend::compiler_messages::NamingConvention::LowercaseWithUnderscores => "lowercase_with_underscores",
                crate::compiler_frontend::compiler_messages::NamingConvention::UppercaseWithUnderscores => "UPPER_CASE_WITH_UNDERSCORES",
                crate::compiler_frontend::compiler_messages::NamingConvention::LowercaseOrUppercaseWithUnderscores => "lowercase_with_underscores or UPPER_CASE_WITH_UNDERSCORES",
            };
            format!(
                "Identifier '{}' should use {}",
                string_table.resolve(*name),
                style_name
            )
        }
        DiagnosticPayload::ImportAliasCaseMismatch { alias, symbol } => {
            format!(
                "Import alias '{}' case mismatch with symbol '{}'",
                string_table.resolve(*alias),
                string_table.resolve(*symbol)
            )
        }
        DiagnosticPayload::MalformedTemplate { message } => {
            format!("Malformed template: {}", string_table.resolve(*message))
        }
        DiagnosticPayload::OldPrefixDeclarationSyntax => {
            "`#` is no longer a declaration prefix".into()
        }
        DiagnosticPayload::InvalidCharacter { character } => {
            format!("Invalid character: '{character}'")
        }
        DiagnosticPayload::InvalidNumberLiteral {
            literal_text,
            reason,
        } => super::invalid_number_literal_message(*literal_text, *reason, string_table),
        DiagnosticPayload::InvalidStyleDirective {
            directive_name,
            supported_directives,
        } => super::invalid_style_directive_message(
            *directive_name,
            *supported_directives,
            string_table,
        ),
        DiagnosticPayload::MissingClosingDelimiter { expected_delimiter } => {
            format!(
                "Missing closing delimiter '{}'",
                string_table.resolve(*expected_delimiter)
            )
        }
        DiagnosticPayload::InvalidGenericApplication { reason } => {
            super::invalid_generic_application_message(*reason).to_owned()
        }
        DiagnosticPayload::UnexpectedEndOfFile { expected_delimiter } => {
            if let Some(expected_delimiter) = expected_delimiter {
                format!(
                    "Unexpected end of file, expected '{}'",
                    string_table.resolve(*expected_delimiter)
                )
            } else {
                "Unexpected end of file".into()
            }
        }
        DiagnosticPayload::InvalidPath { path_kind } => {
            super::invalid_path_message(*path_kind).to_owned()
        }
        DiagnosticPayload::InvalidImportClause { reason, .. } => {
            super::invalid_import_clause_message(*reason).to_owned()
        }
        DiagnosticPayload::InvalidTypeAnnotation { reason, .. } => {
            super::invalid_type_annotation_message(reason, string_table)
        }
        DiagnosticPayload::InvalidCollectionType { reason } => {
            super::invalid_collection_type_message(*reason).to_owned()
        }
        DiagnosticPayload::InvalidGenericParameter { reason } => {
            super::invalid_generic_parameter_message(reason, string_table)
        }
        DiagnosticPayload::InvalidTemplateDirective {
            directive_name,
            reason,
        } => super::invalid_template_directive_message(*directive_name, *reason, string_table),
        DiagnosticPayload::InvalidTemplateStructure { reason } => {
            super::invalid_template_structure_message(*reason)
        }
        DiagnosticPayload::InvalidSignatureMember { reason } => {
            super::invalid_signature_member_message(*reason)
        }
        DiagnosticPayload::InvalidFunctionSignature { reason } => {
            super::invalid_function_signature_message(reason, string_table)
        }
        DiagnosticPayload::InvalidChoiceVariant {
            reason,
            choice_name,
            variant_name,
            available_variants,
        } => super::invalid_choice_variant_message(
            *reason,
            *choice_name,
            *variant_name,
            available_variants,
            string_table,
        ),
        DiagnosticPayload::InvalidStructDefaultValue => "Invalid struct default value".into(),
        DiagnosticPayload::UninitializedVariable { name } => {
            format!("Uninitialized variable '{}'", string_table.resolve(*name))
        }
        DiagnosticPayload::CircularDependency { path } => {
            format!(
                "Circular dependency at '{}'",
                path.to_portable_string(string_table)
            )
        }
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
        } => super::namespace_type_value_misuse_message(*name, *expected, *found, string_table),
        DiagnosticPayload::ShadowedName { name, .. } => {
            format!("Shadowed name '{}'", string_table.resolve(*name))
        }
        DiagnosticPayload::ReservedNameCollision { name, reserved_by } => {
            let owner = match reserved_by {
                crate::compiler_frontend::compiler_messages::ReservedNameOwner::BuiltinType => {
                    "builtin type"
                }
                crate::compiler_frontend::compiler_messages::ReservedNameOwner::Keyword => {
                    "keyword"
                }
            };
            format!(
                "Reserved name collision: '{}' is a reserved {}",
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
            ..
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
            "Cannot infer the element type of an empty collection literal".into()
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
        DiagnosticPayload::None => String::new(),
    }
}

fn import_payload_message(payload: &DiagnosticPayload, string_table: &StringTable) -> String {
    match payload {
        DiagnosticPayload::MissingImportTarget { path } => {
            format!(
                "Missing import target '{}'",
                path.to_portable_string(string_table)
            )
        }
        DiagnosticPayload::AmbiguousImportTarget { path } => {
            format!(
                "Ambiguous import target '{}'",
                path.to_portable_string(string_table)
            )
        }
        DiagnosticPayload::BareFileImport { path } => {
            format!(
                "Bare file import '{}'",
                path.to_portable_string(string_table)
            )
        }
        DiagnosticPayload::DirectSpecialFileImport { path } => {
            format!(
                "Direct special file import '{}'",
                path.to_portable_string(string_table)
            )
        }
        DiagnosticPayload::ImportNameCollision { name, .. } => {
            format!("Import name collision: '{}'", string_table.resolve(*name))
        }
        DiagnosticPayload::NotExportedBySourceFile { symbol_path } => {
            format!(
                "Not exported by source file '{}'",
                symbol_path.to_portable_string(string_table)
            )
        }
        DiagnosticPayload::NotExportedByFacade {
            requested_path,
            facade_name,
            ..
        } => {
            format!(
                "Not exported by facade '{}' for '{}'",
                string_table.resolve(*facade_name),
                requested_path.to_portable_string(string_table)
            )
        }
        DiagnosticPayload::MissingModuleFacade { symbol_path } => {
            format!(
                "Missing #mod.bst facade for module import '{}'",
                symbol_path.to_portable_string(string_table)
            )
        }
        DiagnosticPayload::MissingPackageSymbol {
            symbol,
            package_path,
        } => {
            format!(
                "Missing package symbol '{}' in '{}'",
                string_table.resolve(*symbol),
                string_table.resolve(*package_path)
            )
        }
        DiagnosticPayload::CrossModuleImportNotExported { symbol_path } => {
            format!(
                "Cross-module import not exported '{}'",
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
        DiagnosticPayload::BorrowConflict {
            place,
            existing_access,
            requested_access,
        } => {
            super::borrow_conflict_message(place, *existing_access, *requested_access, string_table)
        }
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

fn sanitize_terse_field(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('|', "/")
}
