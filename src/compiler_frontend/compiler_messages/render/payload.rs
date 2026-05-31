//! Shared diagnostic payload prose.
//!
//! WHAT: owns the primary message and optional guidance for every structured diagnostic payload.
//! WHY: terminal, terse, and dev-server renderers should differ only in output format. The
//! payload facts themselves must become user-facing text through one dispatch path.

use super::*;
use crate::compiler_frontend::compiler_messages::{
    DiagnosticPayload, NamingConvention, ReservedNameOwner,
};

pub(crate) struct RenderedPayload {
    pub(crate) message: String,
    pub(crate) guidance: Vec<String>,
}

pub(crate) fn render_payload(
    payload: &DiagnosticPayload,
    context: DiagnosticRenderContext<'_>,
) -> RenderedPayload {
    let message = render_payload_message(payload, context);
    let guidance = match payload {
        DiagnosticPayload::CommonSyntaxMistake { reason } => {
            vec![format!(
                "Suggestion: {}",
                common_syntax_mistake_suggestion(reason)
            )]
        }
        DiagnosticPayload::TypeMismatch {
            expected, found, ..
        } => vec![
            format!("Expected: {}", diagnostic_type_name(*expected, context)),
            format!("Found: {}", diagnostic_type_name(*found, context)),
        ],
        DiagnosticPayload::CompileTimeEvaluationError { reason, .. } => {
            vec![compile_time_evaluation_error_suggestion(*reason).to_owned()]
        }
        DiagnosticPayload::InvalidGenericInstantiation {
            reason:
                crate::compiler_frontend::compiler_messages::InvalidGenericInstantiationReason::CannotInferFunctionArguments {
                    ..
                },
            ..
        } => {
            vec![
                "Add a type annotation to the receiving declaration, for example `value Int = ...`."
                    .to_owned(),
            ]
        }
        _ => Vec::new(),
    };

    RenderedPayload { message, guidance }
}

fn render_payload_message(
    payload: &DiagnosticPayload,
    context: DiagnosticRenderContext<'_>,
) -> String {
    let string_table = context.string_table;

    match payload {
        DiagnosticPayload::InfrastructureError { msg, .. } => msg.clone(),
        DiagnosticPayload::ExpectedToken { expected, found } => {
            expected_token_message(expected, found.as_ref(), string_table)
        }
        DiagnosticPayload::UnexpectedToken { found } => {
            unexpected_token_message(found, string_table)
        }
        DiagnosticPayload::UnexpectedTrailingComma => "Unexpected trailing comma".to_owned(),
        DiagnosticPayload::UnknownName { name, namespace } => {
            unknown_name_message(*name, *namespace, string_table)
        }
        DiagnosticPayload::TypeMismatch {
            expected,
            found,
            context: mismatch_context,
        } => format!(
            "Type mismatch in {}: expected {}, found {}",
            type_mismatch_context_name(*mismatch_context),
            diagnostic_type_name(*expected, context),
            diagnostic_type_name(*found, context)
        ),
        DiagnosticPayload::DuplicateDeclaration { name, .. } => {
            duplicate_declaration_message(*name, string_table)
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
            invalid_config_message(*key, reason, string_table)
        }
        DiagnosticPayload::DeferredFeature { reason } => {
            deferred_feature_message(reason, string_table)
        }
        DiagnosticPayload::UnsupportedExternalFunction {
            function_name,
            package_path,
            backend_name,
        } => unsupported_external_function_message(
            *function_name,
            *package_path,
            *backend_name,
            string_table,
        ),
        DiagnosticPayload::UnusedName { name } => {
            format!("Unused name '{}'", string_table.resolve(*name))
        }
        DiagnosticPayload::UnreachableMatchArm => "Unreachable match arm".to_owned(),
        DiagnosticPayload::BstFilePathInTemplateOutput { path } => format!(
            "Beanstalk source path '{}' is being inserted into template output",
            string_table.resolve(*path)
        ),
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
        DiagnosticPayload::ImportAliasCaseMismatch { alias, symbol } => format!(
            "Import alias '{}' case mismatch with symbol '{}'",
            string_table.resolve(*alias),
            string_table.resolve(*symbol)
        ),
        DiagnosticPayload::MalformedTemplate { message } => {
            format!("Malformed template: {}", string_table.resolve(*message))
        }
        DiagnosticPayload::OldPrefixDeclarationSyntax => {
            "`#` is no longer a declaration prefix".to_owned()
        }
        DiagnosticPayload::InvalidCharacter { character } => {
            format!("Invalid character: '{character}'")
        }
        DiagnosticPayload::InvalidNumberLiteral {
            literal_text,
            reason,
        } => invalid_number_literal_message(*literal_text, *reason, string_table),
        DiagnosticPayload::InvalidStyleDirective {
            directive_name,
            supported_directives,
        } => invalid_style_directive_message(*directive_name, *supported_directives, string_table),
        DiagnosticPayload::MissingClosingDelimiter { expected_delimiter } => {
            format!(
                "Missing closing delimiter '{}'",
                string_table.resolve(*expected_delimiter)
            )
        }
        DiagnosticPayload::InvalidGenericApplication { reason } => {
            invalid_generic_application_message(*reason).to_owned()
        }
        DiagnosticPayload::UnexpectedEndOfFile { expected_delimiter } => {
            if let Some(expected_delimiter) = expected_delimiter {
                format!(
                    "Unexpected end of file, expected '{}'",
                    string_table.resolve(*expected_delimiter)
                )
            } else {
                "Unexpected end of file".to_owned()
            }
        }
        DiagnosticPayload::InvalidPath { path_kind } => invalid_path_message(*path_kind).to_owned(),
        DiagnosticPayload::InvalidImportClause { reason, .. } => {
            invalid_import_clause_message(*reason).to_owned()
        }
        DiagnosticPayload::InvalidTypeAnnotation { reason, .. } => {
            invalid_type_annotation_message(reason, string_table)
        }
        DiagnosticPayload::InvalidCollectionType { reason } => {
            invalid_collection_type_message(*reason).to_owned()
        }
        DiagnosticPayload::InvalidGenericParameter { reason } => {
            invalid_generic_parameter_message(reason, string_table)
        }
        DiagnosticPayload::InvalidTemplateDirective {
            directive_name,
            reason,
        } => invalid_template_directive_message(*directive_name, *reason, string_table),
        DiagnosticPayload::InvalidTemplateStructure { reason } => {
            invalid_template_structure_message(*reason)
        }
        DiagnosticPayload::InvalidSignatureMember { reason } => {
            invalid_signature_member_message(*reason)
        }
        DiagnosticPayload::InvalidFunctionSignature { reason } => {
            invalid_function_signature_message(reason, string_table)
        }
        DiagnosticPayload::InvalidChoiceVariant {
            reason,
            choice_name,
            variant_name,
            available_variants,
        } => invalid_choice_variant_message(
            *reason,
            *choice_name,
            *variant_name,
            available_variants,
            string_table,
        ),
        DiagnosticPayload::InvalidStructDefaultValue => "Invalid struct default value".to_owned(),
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
        } => namespace_misuse_message(*name, *expected, *found, string_table),
        DiagnosticPayload::ImportRecordUsedAsValue { record_name } => {
            import_record_used_as_value_message(*record_name, string_table)
        }
        DiagnosticPayload::ConstRecordUsedAsValue { record_name } => {
            const_record_used_as_value_message(*record_name, string_table)
        }
        DiagnosticPayload::NestedTraversal { record_name } => {
            nested_traversal_message(*record_name, string_table)
        }
        DiagnosticPayload::NamespaceTypeValueMisuse {
            name,
            expected,
            found,
        } => namespace_type_value_misuse_message(*name, *expected, *found, string_table),
        DiagnosticPayload::ShadowedName { name, .. } => {
            format!("Shadowed name '{}'", string_table.resolve(*name))
        }
        DiagnosticPayload::ReservedNameCollision { name, reserved_by } => {
            let owner = match reserved_by {
                ReservedNameOwner::BuiltinType => "builtin type",
                ReservedNameOwner::Keyword => "keyword",
            };
            format!(
                "Reserved name collision: '{}' is a reserved {}",
                string_table.resolve(*name),
                owner
            )
        }
        DiagnosticPayload::InvalidThisUsage { reason } => {
            invalid_this_usage_message(*reason, string_table)
        }
        DiagnosticPayload::InvalidReceiverDeclaration { reason } => {
            invalid_receiver_declaration_message(*reason, string_table)
        }
        DiagnosticPayload::InvalidControlFlowStatement { reason } => {
            invalid_control_flow_statement_message(*reason)
        }
        DiagnosticPayload::InvalidDeclaration { reason, name } => {
            invalid_declaration_message(reason.clone(), *name, string_table)
        }
        DiagnosticPayload::InvalidAssignmentTarget {
            reason,
            target_name,
            target_type,
        } => invalid_assignment_target_message(*reason, *target_name, *target_type, context),
        DiagnosticPayload::InvalidMultiBind {
            reason,
            target_name,
        } => invalid_multi_bind_message(*reason, *target_name, string_table),
        DiagnosticPayload::InvalidBuiltinCall {
            reason,
            builtin_name,
        } => invalid_builtin_call_message(*reason, *builtin_name, string_table),
        DiagnosticPayload::InvalidReceiverCall {
            reason,
            receiver_type,
            method_name,
        } => invalid_receiver_call_message(*reason, *receiver_type, *method_name, string_table),
        DiagnosticPayload::InvalidCopyTarget { reason } => invalid_copy_target_message(*reason),
        DiagnosticPayload::InvalidFieldAccess {
            reason,
            field_name,
            receiver_type,
        } => invalid_field_access_message(*reason, *field_name, *receiver_type, context),
        DiagnosticPayload::InvalidMatchPattern {
            reason,
            variant_name,
            ..
        } => invalid_match_pattern_message(*reason, *variant_name, string_table),
        DiagnosticPayload::NonExhaustiveMatch {
            reason,
            missing_variants,
            ..
        } => non_exhaustive_match_message(*reason, missing_variants, string_table),
        DiagnosticPayload::InvalidResultHandling { reason } => reason.message().to_owned(),
        DiagnosticPayload::InvalidTemplateSlot { reason, slot_name } => {
            invalid_template_slot_message(*reason, *slot_name, string_table)
        }
        DiagnosticPayload::CompileTimeEvaluationError { reason, operation } => {
            compile_time_evaluation_error_message(*reason, *operation, string_table)
        }
        DiagnosticPayload::EmptyCollectionTypeAmbiguity => {
            "Cannot infer the element type of an empty collection literal".to_owned()
        }
        DiagnosticPayload::UnsupportedOperatorTypes { category, lhs, rhs } => {
            unsupported_operator_types_message(*category, *lhs, *rhs, context)
        }
        DiagnosticPayload::InvalidResultOperand {
            reason,
            category,
            operand_type,
        } => invalid_result_operand_message(*reason, *category, *operand_type, context),
        DiagnosticPayload::IncompatibleChoiceComparison { reason, lhs, rhs } => {
            incompatible_choice_comparison_message(reason, *lhs, *rhs, context)
        }
        DiagnosticPayload::InvalidCallShape { reason, .. } => {
            invalid_call_shape_message(reason.clone())
        }
        DiagnosticPayload::InvalidReturnShape { reason } => invalid_return_shape_message(*reason),
        DiagnosticPayload::InvalidGenericInstantiation { type_name, reason } => {
            invalid_generic_instantiation_message(*type_name, reason, context)
        }
        DiagnosticPayload::InvalidRangeOperand {
            operand,
            found_type,
        } => invalid_range_operand_message(*operand, *found_type, context),
        DiagnosticPayload::UnsupportedBuilderPackage { package_path } => {
            unsupported_builder_package_message(*package_path, string_table)
        }
        DiagnosticPayload::UnsupportedBackendFeature {
            backend_name,
            feature,
        } => format!(
            "Backend '{}' does not support {} yet.",
            string_table.resolve(*backend_name),
            string_table.resolve(*feature)
        ),
        DiagnosticPayload::InvalidPageMetadata { key, reason } => {
            invalid_page_metadata_message(*key, *reason, string_table)
        }
        DiagnosticPayload::InvalidCompileTimePath { path, reason } => {
            invalid_compile_time_path_message(path, *reason, string_table)
        }
        DiagnosticPayload::InvalidExpression => invalid_expression_message(),
        DiagnosticPayload::CommonSyntaxMistake { reason } => {
            common_syntax_mistake_message(reason, string_table)
        }
        DiagnosticPayload::MissingOperatorOperand { operator, position } => {
            missing_operator_operand_message(*operator, *position, string_table)
        }
        DiagnosticPayload::InvalidStandaloneStatement { reason } => {
            invalid_standalone_statement_message(*reason)
        }
        DiagnosticPayload::ExpectedSymbolStatement => expected_symbol_statement_message(),
        DiagnosticPayload::MissingCollectionItem => missing_collection_item_message(),
        DiagnosticPayload::InvalidMatchArm { reason } => invalid_match_arm_message(*reason),
        DiagnosticPayload::InvalidLoopHeader { reason } => {
            invalid_loop_header_message(*reason, context)
        }
        DiagnosticPayload::InvalidStatementPosition { reason } => {
            invalid_statement_position_message(*reason)
        }
        DiagnosticPayload::None => String::new(),
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
        DiagnosticPayload::AmbiguousImportTarget { path } => format!(
            "Ambiguous import target '{}'. Use a more specific path.",
            path.to_portable_string(string_table)
        ),
        DiagnosticPayload::BareFileImport { path } => format!(
            "Bare file imports are not supported; import an exported symbol from the file '{}'.",
            path.to_portable_string(string_table)
        ),
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
            let path_text = requested_path.to_portable_string(string_table);
            let facade_name = string_table.resolve(*facade_name);
            match facade_type {
                crate::compiler_frontend::compiler_messages::ImportFacadeType::SourceLibrary => {
                    format!(
                        "Cannot import '{path_text}' from source library '@{facade_name}' because it is not exported by the library facade."
                    )
                }
                crate::compiler_frontend::compiler_messages::ImportFacadeType::ModuleRoot => {
                    format!(
                        "Cannot import '{path_text}' from module '{facade_name}' because it is not exported by the module's facade."
                    )
                }
            }
        }
        DiagnosticPayload::MissingModuleFacade { symbol_path } => format!(
            "Cannot import '{}' because the target module has no #mod.bst facade. Import a concrete file from inside the same module, or add #mod.bst to define the module's public import surface.",
            symbol_path.to_portable_string(string_table)
        ),
        DiagnosticPayload::MissingPackageSymbol {
            symbol,
            package_path,
        } => format!(
            "Cannot import '{}' from package '{}': symbol not found.",
            string_table.resolve(*symbol),
            string_table.resolve(*package_path)
        ),
        DiagnosticPayload::CrossModuleImportNotExported { symbol_path } => format!(
            "Cannot import '{}' because it is not exported by the target module's facade.",
            symbol_path.to_portable_string(string_table)
        ),
        DiagnosticPayload::InvalidImportPath { path, reason } => {
            invalid_import_path_message(path, *reason, string_table)
        }
        DiagnosticPayload::DirectSymbolPathImport { path } => {
            direct_symbol_path_import_message(path, string_table)
        }
        DiagnosticPayload::InvalidNamespaceDefaultName { path } => {
            invalid_namespace_default_name_message(path, string_table)
        }
        DiagnosticPayload::DuplicateImportSurfaceMember {
            surface_path,
            member_name,
        } => duplicate_import_surface_member_message(surface_path, *member_name, string_table),
        DiagnosticPayload::ExplicitBstExtension { path } => {
            explicit_bst_extension_message(path, string_table)
        }
        DiagnosticPayload::UnsupportedExternalExtension { path, extension } => {
            unsupported_external_extension_message(path, *extension, string_table)
        }
        DiagnosticPayload::InvalidExternalLibrary { path, message } => {
            invalid_external_library_message(path, *message, string_table)
        }
        DiagnosticPayload::ReceiverMethodImportRequiresVisibleReceiverType {
            method_name,
            receiver_type_name,
        } => receiver_method_import_requires_visible_receiver_type_message(
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
            ..
        } => borrow_conflict_message(place, *existing_access, *requested_access, string_table),
        DiagnosticPayload::MultipleMutableBorrows { place, .. } => {
            multiple_mutable_borrows_message(place, string_table)
        }
        DiagnosticPayload::SharedMutableConflict {
            place,
            existing_access,
            requested_access,
            conflicting_place,
            ..
        } => shared_mutable_conflict_message(
            place,
            *existing_access,
            *requested_access,
            conflicting_place.as_ref(),
            string_table,
        ),
        DiagnosticPayload::UseAfterPossibleMove { place, .. } => {
            use_after_possible_move_message(place, string_table)
        }
        DiagnosticPayload::MoveWhileBorrowed {
            place,
            existing_access,
            ..
        } => move_while_borrowed_message(place, *existing_access, string_table),
        DiagnosticPayload::WholeObjectBorrowConflict {
            whole_place,
            part_place,
            ..
        } => whole_object_borrow_conflict_message(whole_place, part_place, string_table),
        DiagnosticPayload::InvalidMutableAccess {
            place,
            reason,
            conflicting_place,
        } => {
            invalid_mutable_access_message(place, *reason, conflicting_place.as_ref(), string_table)
        }
        DiagnosticPayload::InvalidAccessAfterPossibleOwnershipTransfer { place } => {
            invalid_access_after_possible_ownership_transfer_message(place, string_table)
        }
        DiagnosticPayload::UseOfUninitializedLocal { place } => {
            use_of_uninitialized_local_message(place, string_table)
        }
        _ => String::new(),
    }
}
