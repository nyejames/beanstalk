//! Terminal rendering for `CompilerDiagnostic`.
//!
//! WHAT: converts structured diagnostics into coloured terminal output.
//! WHY: this is the primary human-facing render path for compiler errors and warnings.

use crate::compiler_frontend::compiler_messages::render::{
    DiagnosticRenderContext, diagnostic_type_name, display_column_number, display_line_number,
    relative_display_path_from_root, resolve_source_file_path, special_file_name_from_path,
    type_mismatch_context_name,
};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticLabelMessage, DiagnosticLabelStyle, DiagnosticPayload,
    DiagnosticSeverity, NamingConvention,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use saying::say;
use std::fs;

pub(crate) fn print_diagnostics(diagnostics: &[CompilerDiagnostic], string_table: &StringTable) {
    let context = DiagnosticRenderContext::new(string_table);
    print_diagnostics_with_context(diagnostics, context);
}

pub(crate) fn print_diagnostics_with_context(
    diagnostics: &[CompilerDiagnostic],
    context: DiagnosticRenderContext<'_>,
) {
    for diagnostic in diagnostics {
        print_diagnostic_with_context(diagnostic, context);
    }
}

pub(crate) fn print_diagnostic(diagnostic: &CompilerDiagnostic, string_table: &StringTable) {
    let context = DiagnosticRenderContext::new(string_table);
    print_diagnostic_with_context(diagnostic, context);
}

pub(crate) fn print_diagnostic_with_context(
    diagnostic: &CompilerDiagnostic,
    context: DiagnosticRenderContext<'_>,
) {
    let string_table = context.string_table;
    let descriptor = diagnostic.kind.descriptor();
    let severity_name = severity_display_name(diagnostic.severity);
    let visual = severity_visual(diagnostic.severity);

    match diagnostic.severity {
        DiagnosticSeverity::Error => {
            say!("\n", Bright Bold Red severity_name, Reset " ", Reset visual);
        }
        DiagnosticSeverity::Warning => {
            say!("\n", Bright Bold Yellow severity_name, Reset " ", Reset visual);
        }
        DiagnosticSeverity::Note => {
            say!("\n", Bright Bold Blue severity_name, Reset " ", Reset visual);
        }
    }

    // Title and code
    say!(Reset descriptor.title);
    say!(Dark "  [", descriptor.code, "]");

    // Location
    let relative_dir = relative_display_path_from_root(
        &resolve_source_file_path(&diagnostic.primary_location.scope, string_table),
        &std::env::current_dir().unwrap_or_default(),
    );
    let display_line = display_line_number(diagnostic.primary_location.start_pos.line_number);
    let display_column = display_column_number(diagnostic.primary_location.start_pos.char_column);

    if !relative_dir.is_empty() {
        say!(
            Blue "\n  --> ",
            Reset Magenta relative_dir.as_str(),
            Dark Magenta ":",
            Reset Bold Blue display_line,
            Reset Grey ":",
            Reset Magenta display_column
        );
    } else {
        say!(
            Blue "\n   --> ",
            Reset Magenta display_line,
            Dark Magenta ":",
            Reset Magenta display_column
        );
    }

    // Source snippet
    let actual_file = resolve_source_file_path(&diagnostic.primary_location.scope, string_table);
    let source_line_index = diagnostic.primary_location.start_pos.line_number.max(0) as usize;
    let line = match fs::read_to_string(&actual_file) {
        Ok(file) => file
            .lines()
            .nth(source_line_index)
            .unwrap_or_default()
            .to_string(),
        Err(_) => String::new(),
    };

    if !line.is_empty() {
        say!(Blue "    |");
        let line_label = display_line.to_string();
        let line_padding = " ".repeat(3usize.saturating_sub(line_label.len()));
        say!(Blue line_padding, Bold Blue line_label, " | ", Reset line.as_str());
        print!("{}", " ".repeat(display_line.to_string().len() + 4));

        let underline_start = diagnostic.primary_location.start_pos.char_column.max(0) as usize;
        print!("{}", " ".repeat(underline_start));
        let underline_length = (diagnostic.primary_location.end_pos.char_column
            - diagnostic.primary_location.start_pos.char_column
            + 1)
        .max(1) as usize;
        say!(Red "^".repeat(underline_length));
    }

    // Labels
    for label_message in format_label_messages(diagnostic, string_table) {
        say!(Bright Blue "  ", label_message);
    }

    // Payload-specific guidance
    for guidance in format_payload_guidance(&diagnostic.payload, context) {
        say!(Bright Blue "  ", guidance);
    }

    if line.is_empty() && diagnostic.primary_location.scope.as_components().is_empty() {
        say!(Dark "     No source location available.");
    }
}

pub(crate) fn format_label_messages(
    diagnostic: &CompilerDiagnostic,
    string_table: &StringTable,
) -> Vec<String> {
    let mut rendered_labels = Vec::new();

    for label in &diagnostic.labels {
        if let Some(message) = &label.message {
            let label_line = display_line_number(label.location.start_pos.line_number);
            let label_col = display_column_number(label.location.start_pos.char_column);
            let style_name = match label.style {
                DiagnosticLabelStyle::Primary => "note",
                DiagnosticLabelStyle::Secondary => "info",
            };
            let message_text = diagnostic_label_message_text(message, string_table);

            rendered_labels.push(format!(
                "{style_name}: {label_line}:{label_col} — {message_text}"
            ));
        }
    }

    rendered_labels
}

fn diagnostic_label_message_text(
    message: &DiagnosticLabelMessage,
    string_table: &StringTable,
) -> String {
    match message {
        DiagnosticLabelMessage::PreviousDeclaration => "previous declaration here".to_owned(),
        DiagnosticLabelMessage::ExistingBorrow => "existing borrow here".to_owned(),
        DiagnosticLabelMessage::ExpectedTypeDeclaredHere => {
            "expected type declared here".to_owned()
        }
        DiagnosticLabelMessage::ValueMovedHere => "value moved here".to_owned(),
        DiagnosticLabelMessage::RenderedText(text) => string_table.resolve(*text).to_owned(),
        DiagnosticLabelMessage::GenericInstantiationCallSite => {
            "while instantiating this generic call".to_owned()
        }
        DiagnosticLabelMessage::GenericInstantiationBodySite => {
            "generic body operation failed here".to_owned()
        }
    }
}

fn severity_display_name(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "Error",
        DiagnosticSeverity::Warning => "Warning",
        DiagnosticSeverity::Note => "Note",
    }
}

fn severity_visual(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "(╯°□°)╯ 🔥",
        DiagnosticSeverity::Warning => "⚠️",
        DiagnosticSeverity::Note => "📝",
    }
}

pub(crate) fn format_payload_guidance(
    payload: &DiagnosticPayload,
    context: DiagnosticRenderContext<'_>,
) -> Vec<String> {
    let string_table = context.string_table;
    let mut lines = Vec::new();

    match payload {
        DiagnosticPayload::InfrastructureError { msg, .. } => {
            lines.push(msg.clone());
        }
        DiagnosticPayload::ExpectedToken { expected, found } => {
            lines.push(super::expected_token_message(
                expected,
                found.as_ref(),
                string_table,
            ));
        }
        DiagnosticPayload::TypeMismatch {
            expected,
            found,
            context: mismatch_context,
        } => {
            lines.push(format!(
                "Type mismatch in {} context.",
                type_mismatch_context_name(*mismatch_context)
            ));
            lines.push(format!(
                "Expected: {}",
                diagnostic_type_name(*expected, context)
            ));
            lines.push(format!("Found: {}", diagnostic_type_name(*found, context)));
        }
        DiagnosticPayload::UnknownName { name, namespace } => {
            lines.push(super::unknown_name_message(*name, *namespace, string_table));
        }
        DiagnosticPayload::DuplicateDeclaration { name, .. } => {
            lines.push(super::duplicate_declaration_message(*name, string_table));
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
            lines.extend(format_import_payload_guidance(payload, string_table));
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
            lines.extend(format_borrow_payload_guidance(payload, string_table));
        }
        DiagnosticPayload::InvalidConfig { key, reason } => {
            lines.push(super::invalid_config_message(*key, reason, string_table));
        }
        DiagnosticPayload::DeferredFeature { reason } => {
            lines.push(super::deferred_feature_message(reason, string_table));
        }
        DiagnosticPayload::UnsupportedExternalFunction {
            function_name,
            package_path,
            backend_name,
        } => {
            lines.push(super::unsupported_external_function_message(
                *function_name,
                *package_path,
                *backend_name,
                string_table,
            ));
        }
        DiagnosticPayload::UnusedName { name } => {
            lines.push(format!("Unused name '{}'", string_table.resolve(*name)));
        }
        DiagnosticPayload::UnreachableMatchArm => {
            lines.push("This match arm is unreachable".into());
        }
        DiagnosticPayload::BstFilePathInTemplateOutput { path } => {
            lines.push(format!(
                "Beanstalk source path '{}' is being inserted into template output",
                string_table.resolve(*path)
            ));
        }
        DiagnosticPayload::LargeTrackedAsset { path, byte_size } => {
            let mib = *byte_size as f64 / (1024.0 * 1024.0);
            lines.push(format!(
                "Large tracked asset '{}' ({mib:.1} MiB)",
                string_table.resolve(*path)
            ));
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
            lines.push(format!(
                "Identifier '{}' should use {}",
                string_table.resolve(*name),
                style_name
            ));
        }
        DiagnosticPayload::ImportAliasCaseMismatch { alias, symbol } => {
            lines.push(format!(
                "Import alias '{}' uses different leading-name case than imported symbol '{}'",
                string_table.resolve(*alias),
                string_table.resolve(*symbol)
            ));
        }
        DiagnosticPayload::MalformedTemplate { message } => {
            lines.push(format!(
                "Malformed template: {}",
                string_table.resolve(*message)
            ));
        }
        DiagnosticPayload::OldPrefixDeclarationSyntax => {
            lines.push("`#` is no longer a declaration prefix.".into());
            lines.push(
                "Use `name #= value` for inferred compile-time constants or `name #Type = value` for explicit constant types.".into(),
            );
            lines.push(
                "Module visibility is controlled by file/module boundaries and `#mod.bst` facades."
                    .into(),
            );
        }
        DiagnosticPayload::UnexpectedToken { found } => {
            lines.push(super::unexpected_token_message(found, string_table));
        }
        DiagnosticPayload::UnexpectedTrailingComma => {
            lines.push("Unexpected trailing comma".into());
        }
        DiagnosticPayload::InvalidCharacter { character } => {
            lines.push(format!("Invalid character: '{character}'"));
        }
        DiagnosticPayload::InvalidNumberLiteral {
            literal_text,
            reason,
        } => {
            lines.push(super::invalid_number_literal_message(
                *literal_text,
                *reason,
                string_table,
            ));
        }
        DiagnosticPayload::InvalidStyleDirective {
            directive_name,
            supported_directives,
        } => {
            lines.push(super::invalid_style_directive_message(
                *directive_name,
                *supported_directives,
                string_table,
            ));
        }
        DiagnosticPayload::MissingClosingDelimiter { expected_delimiter } => {
            lines.push(format!(
                "Missing closing delimiter '{}'",
                string_table.resolve(*expected_delimiter)
            ));
        }
        DiagnosticPayload::InvalidGenericApplication { reason } => {
            lines.push(super::invalid_generic_application_message(*reason).to_owned());
        }
        DiagnosticPayload::UnexpectedEndOfFile { expected_delimiter } => {
            if let Some(expected_delimiter) = expected_delimiter {
                lines.push(format!(
                    "Unexpected end of file, expected '{}'",
                    string_table.resolve(*expected_delimiter)
                ));
            } else {
                lines.push("Unexpected end of file".into());
            }
        }
        DiagnosticPayload::InvalidPath { path_kind } => {
            lines.push(super::invalid_path_message(*path_kind).to_owned());
        }
        DiagnosticPayload::InvalidImportClause { reason, .. } => {
            lines.push(super::invalid_import_clause_message(*reason).to_owned());
        }
        DiagnosticPayload::InvalidTypeAnnotation { reason, .. } => {
            lines.push(super::invalid_type_annotation_message(reason, string_table));
        }
        DiagnosticPayload::InvalidCollectionType { reason } => {
            lines.push(super::invalid_collection_type_message(*reason).to_owned());
        }
        DiagnosticPayload::InvalidGenericParameter { reason } => {
            lines.push(super::invalid_generic_parameter_message(
                reason,
                string_table,
            ));
        }
        DiagnosticPayload::InvalidTemplateDirective {
            directive_name,
            reason,
        } => {
            lines.push(super::invalid_template_directive_message(
                *directive_name,
                *reason,
                string_table,
            ));
        }
        DiagnosticPayload::InvalidTemplateStructure { reason } => {
            lines.push(super::invalid_template_structure_message(*reason));
        }
        DiagnosticPayload::InvalidSignatureMember { reason } => {
            lines.push(super::invalid_signature_member_message(*reason));
        }
        DiagnosticPayload::InvalidFunctionSignature { reason } => {
            lines.push(super::invalid_function_signature_message(
                reason,
                string_table,
            ));
        }
        DiagnosticPayload::InvalidChoiceVariant {
            reason,
            choice_name,
            variant_name,
            available_variants,
        } => {
            lines.push(super::invalid_choice_variant_message(
                *reason,
                *choice_name,
                *variant_name,
                available_variants,
                string_table,
            ));
        }
        DiagnosticPayload::InvalidStructDefaultValue => {
            lines.push("Invalid struct default value".into());
        }
        DiagnosticPayload::UninitializedVariable { name } => {
            lines.push(format!(
                "Uninitialized variable '{}'",
                string_table.resolve(*name)
            ));
        }
        DiagnosticPayload::CircularDependency { path } => {
            lines.push(format!(
                "Circular dependency at '{}'",
                path.to_portable_string(string_table)
            ));
        }
        DiagnosticPayload::NamespaceMisuse {
            name,
            expected,
            found,
        } => {
            lines.push(super::namespace_misuse_message(
                *name,
                *expected,
                *found,
                string_table,
            ));
        }
        DiagnosticPayload::ImportRecordUsedAsValue { record_name } => {
            lines.push(super::import_record_used_as_value_message(
                *record_name,
                string_table,
            ));
        }
        DiagnosticPayload::ConstRecordUsedAsValue { record_name } => {
            lines.push(super::const_record_used_as_value_message(
                *record_name,
                string_table,
            ));
        }
        DiagnosticPayload::NestedTraversal { record_name } => {
            lines.push(super::nested_traversal_message(*record_name, string_table));
        }
        DiagnosticPayload::NamespaceTypeValueMisuse {
            name,
            expected,
            found,
        } => {
            lines.push(super::namespace_type_value_misuse_message(
                *name,
                *expected,
                *found,
                string_table,
            ));
        }
        DiagnosticPayload::ShadowedName { name, .. } => {
            lines.push(format!(
                "Name '{}' was previously declared in this scope",
                string_table.resolve(*name)
            ));
        }
        DiagnosticPayload::ReservedNameCollision { name, reserved_by } => {
            let owner = match reserved_by {
                crate::compiler_frontend::compiler_messages::ReservedNameOwner::BuiltinType => {
                    "builtin type"
                }
                crate::compiler_frontend::compiler_messages::ReservedNameOwner::Keyword => {
                    "language keyword"
                }
            };
            lines.push(format!(
                "Name '{}' collides with a reserved {}",
                string_table.resolve(*name),
                owner
            ));
        }
        DiagnosticPayload::InvalidThisUsage { reason } => {
            lines.push(super::invalid_this_usage_message(*reason, string_table));
        }
        DiagnosticPayload::InvalidReceiverDeclaration { reason } => {
            lines.push(super::invalid_receiver_declaration_message(
                *reason,
                string_table,
            ));
        }
        DiagnosticPayload::InvalidControlFlowStatement { reason } => {
            lines.push(super::invalid_control_flow_statement_message(*reason));
        }
        DiagnosticPayload::InvalidDeclaration { reason, name } => {
            lines.push(super::invalid_declaration_message(
                reason.clone(),
                *name,
                string_table,
            ));
        }
        DiagnosticPayload::InvalidAssignmentTarget {
            reason,
            target_name,
            target_type,
        } => {
            lines.push(super::invalid_assignment_target_message(
                *reason,
                *target_name,
                *target_type,
                context,
            ));
        }
        DiagnosticPayload::InvalidMultiBind {
            reason,
            target_name,
        } => {
            lines.push(super::invalid_multi_bind_message(
                *reason,
                *target_name,
                string_table,
            ));
        }
        DiagnosticPayload::InvalidBuiltinCall {
            reason,
            builtin_name,
        } => {
            lines.push(super::invalid_builtin_call_message(
                *reason,
                *builtin_name,
                string_table,
            ));
        }
        DiagnosticPayload::InvalidReceiverCall {
            reason,
            receiver_type,
            method_name,
        } => {
            lines.push(super::invalid_receiver_call_message(
                *reason,
                *receiver_type,
                *method_name,
                string_table,
            ));
        }
        DiagnosticPayload::InvalidCopyTarget { reason } => {
            lines.push(super::invalid_copy_target_message(*reason));
        }
        DiagnosticPayload::InvalidFieldAccess {
            reason,
            field_name,
            receiver_type,
        } => {
            lines.push(super::invalid_field_access_message(
                *reason,
                *field_name,
                *receiver_type,
                context,
            ));
        }
        DiagnosticPayload::InvalidMatchPattern {
            reason,
            variant_name,
            scrutinee_name: _,
        } => {
            lines.push(super::invalid_match_pattern_message(
                *reason,
                *variant_name,
                string_table,
            ));
        }
        DiagnosticPayload::NonExhaustiveMatch {
            reason,
            missing_variants,
            ..
        } => {
            lines.push(super::non_exhaustive_match_message(
                *reason,
                missing_variants,
                string_table,
            ));
        }
        DiagnosticPayload::InvalidResultHandling { reason } => {
            lines.push(reason.message().to_string());
        }
        DiagnosticPayload::InvalidTemplateSlot { reason, slot_name } => {
            lines.push(super::invalid_template_slot_message(
                *reason,
                *slot_name,
                string_table,
            ));
        }
        DiagnosticPayload::CompileTimeEvaluationError { reason, operation } => {
            lines.push(super::compile_time_evaluation_error_message(
                *reason,
                *operation,
                string_table,
            ));
            lines.push(super::compile_time_evaluation_error_suggestion(*reason).to_string());
        }
        DiagnosticPayload::EmptyCollectionTypeAmbiguity => {
            lines.push("Cannot infer the element type of an empty collection literal.".to_string());
            lines.push(
                "Empty collections require an explicit collection type annotation.".to_string(),
            );
        }
        DiagnosticPayload::UnsupportedOperatorTypes { category, lhs, rhs } => {
            lines.push(super::unsupported_operator_types_message(
                *category, *lhs, *rhs, context,
            ));
        }
        DiagnosticPayload::InvalidResultOperand {
            reason,
            category,
            operand_type,
        } => {
            lines.push(super::invalid_result_operand_message(
                *reason,
                *category,
                *operand_type,
                context,
            ));
            lines.push("Handle the value with explicit unwrapping before using it in an ordinary expression.".to_string());
        }
        DiagnosticPayload::IncompatibleChoiceComparison { reason, lhs, rhs } => {
            lines.push(super::incompatible_choice_comparison_message(
                reason, *lhs, *rhs, context,
            ));
        }
        DiagnosticPayload::InvalidCallShape { reason, .. } => {
            lines.push(super::invalid_call_shape_message(reason.clone()));
        }
        DiagnosticPayload::InvalidReturnShape { reason } => {
            lines.push(super::invalid_return_shape_message(*reason));
        }
        DiagnosticPayload::InvalidGenericInstantiation { type_name, reason } => {
            lines.push(super::invalid_generic_instantiation_message(
                *type_name,
                reason,
                string_table,
            ));
        }
        DiagnosticPayload::InvalidRangeOperand {
            operand,
            found_type,
        } => {
            lines.push(super::invalid_range_operand_message(
                *operand,
                *found_type,
                context,
            ));
        }
        DiagnosticPayload::UnsupportedBuilderPackage { package_path } => {
            lines.push(super::unsupported_builder_package_message(
                *package_path,
                string_table,
            ));
        }
        DiagnosticPayload::InvalidPageMetadata { key, reason } => {
            lines.push(super::invalid_page_metadata_message(
                *key,
                *reason,
                string_table,
            ));
        }
        DiagnosticPayload::InvalidCompileTimePath { path, reason } => {
            lines.push(super::invalid_compile_time_path_message(
                path,
                *reason,
                string_table,
            ));
        }
        DiagnosticPayload::InvalidExpression => {
            lines.push(super::invalid_expression_message());
        }
        DiagnosticPayload::CommonSyntaxMistake { reason } => {
            lines.push(super::common_syntax_mistake_message(reason, string_table));
            lines.push(format!(
                "Suggestion: {}",
                super::common_syntax_mistake_suggestion(reason)
            ));
        }
        DiagnosticPayload::MissingOperatorOperand { operator, position } => {
            lines.push(super::missing_operator_operand_message(
                *operator,
                *position,
                string_table,
            ));
        }
        DiagnosticPayload::InvalidStandaloneStatement { reason } => {
            lines.push(super::invalid_standalone_statement_message(*reason));
        }
        DiagnosticPayload::ExpectedSymbolStatement => {
            lines.push(super::expected_symbol_statement_message());
        }
        DiagnosticPayload::MissingCollectionItem => {
            lines.push(super::missing_collection_item_message());
        }
        DiagnosticPayload::InvalidMatchArm { reason } => {
            lines.push(super::invalid_match_arm_message(*reason));
        }
        DiagnosticPayload::InvalidLoopHeader { reason } => {
            lines.push(super::invalid_loop_header_message(*reason, context));
        }
        DiagnosticPayload::InvalidStatementPosition { reason } => {
            lines.push(super::invalid_statement_position_message(*reason));
        }
        DiagnosticPayload::None => {}
    }

    lines
}

fn format_import_payload_guidance(
    payload: &DiagnosticPayload,
    string_table: &StringTable,
) -> Vec<String> {
    match payload {
        DiagnosticPayload::MissingImportTarget { path } => {
            vec![format!(
                "Cannot resolve import '{}'.",
                path.to_portable_string(string_table)
            )]
        }
        DiagnosticPayload::AmbiguousImportTarget { path } => {
            vec![format!(
                "Ambiguous import target '{}'. Use a more specific path.",
                path.to_portable_string(string_table)
            )]
        }
        DiagnosticPayload::BareFileImport { path } => {
            vec![format!(
                "Bare file imports are not supported; import an exported symbol from the file '{}'.",
                path.to_portable_string(string_table)
            )]
        }
        DiagnosticPayload::DirectSpecialFileImport { path } => {
            let special_file = special_file_name_from_path(path, string_table);
            vec![format!(
                "Cannot import directly from '{special_file}' via '{}'. Import exported symbols through the module path instead.",
                path.to_portable_string(string_table)
            )]
        }
        DiagnosticPayload::ImportNameCollision { name, .. } => {
            vec![format!(
                "Import name collision: '{}' is already visible in this file.",
                string_table.resolve(*name)
            )]
        }
        DiagnosticPayload::NotExportedBySourceFile { symbol_path } => {
            vec![format!(
                "Cannot import '{}' because it is not exported.",
                symbol_path.to_portable_string(string_table)
            )]
        }
        DiagnosticPayload::NotExportedByFacade {
            requested_path,
            facade_name,
            facade_type,
        } => {
            let path_str = requested_path.to_portable_string(string_table);
            let facade_name_str = string_table.resolve(*facade_name);
            let msg = match facade_type {
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
            };
            vec![msg]
        }
        DiagnosticPayload::MissingModuleFacade { symbol_path } => {
            vec![format!(
                "Cannot import '{}' because the target module has no #mod.bst facade. Import a concrete file from inside the same module, or add #mod.bst to define the module's public import surface.",
                symbol_path.to_portable_string(string_table)
            )]
        }
        DiagnosticPayload::MissingPackageSymbol {
            symbol,
            package_path,
        } => {
            vec![format!(
                "Cannot import '{}' from package '{}': symbol not found.",
                string_table.resolve(*symbol),
                string_table.resolve(*package_path)
            )]
        }
        DiagnosticPayload::CrossModuleImportNotExported { symbol_path } => {
            vec![format!(
                "Cannot import '{}' because it is not exported by the target module's facade.",
                symbol_path.to_portable_string(string_table)
            )]
        }
        DiagnosticPayload::InvalidImportPath { path, reason } => {
            vec![super::invalid_import_path_message(
                path,
                *reason,
                string_table,
            )]
        }
        DiagnosticPayload::DirectSymbolPathImport { path } => {
            vec![super::direct_symbol_path_import_message(path, string_table)]
        }
        DiagnosticPayload::InvalidNamespaceDefaultName { path } => {
            vec![super::invalid_namespace_default_name_message(
                path,
                string_table,
            )]
        }
        DiagnosticPayload::DuplicateImportSurfaceMember {
            surface_path,
            member_name,
        } => {
            vec![super::duplicate_import_surface_member_message(
                surface_path,
                *member_name,
                string_table,
            )]
        }
        DiagnosticPayload::ExplicitBstExtension { path } => {
            vec![super::explicit_bst_extension_message(path, string_table)]
        }
        DiagnosticPayload::UnsupportedExternalExtension { path, extension } => {
            vec![super::unsupported_external_extension_message(
                path,
                *extension,
                string_table,
            )]
        }
        DiagnosticPayload::InvalidExternalLibrary { path, message } => {
            vec![super::invalid_external_library_message(
                path,
                *message,
                string_table,
            )]
        }
        DiagnosticPayload::ReceiverMethodImportRequiresVisibleReceiverType {
            method_name,
            receiver_type_name,
        } => {
            vec![
                super::receiver_method_import_requires_visible_receiver_type_message(
                    *method_name,
                    *receiver_type_name,
                    string_table,
                ),
            ]
        }
        _ => Vec::new(),
    }
}

fn format_borrow_payload_guidance(
    payload: &DiagnosticPayload,
    string_table: &StringTable,
) -> Vec<String> {
    match payload {
        DiagnosticPayload::BorrowConflict {
            place,
            existing_access,
            requested_access,
        } => {
            vec![super::borrow_conflict_message(
                place,
                *existing_access,
                *requested_access,
                string_table,
            )]
        }
        DiagnosticPayload::MultipleMutableBorrows { place, .. } => {
            vec![super::multiple_mutable_borrows_message(place, string_table)]
        }
        DiagnosticPayload::SharedMutableConflict {
            place,
            existing_access,
            requested_access,
            conflicting_place,
            ..
        } => {
            vec![super::shared_mutable_conflict_message(
                place,
                *existing_access,
                *requested_access,
                conflicting_place.as_ref(),
                string_table,
            )]
        }
        DiagnosticPayload::UseAfterPossibleMove { place, .. } => {
            vec![super::use_after_possible_move_message(place, string_table)]
        }
        DiagnosticPayload::MoveWhileBorrowed {
            place,
            existing_access,
            ..
        } => {
            vec![super::move_while_borrowed_message(
                place,
                *existing_access,
                string_table,
            )]
        }
        DiagnosticPayload::WholeObjectBorrowConflict {
            whole_place,
            part_place,
            ..
        } => {
            vec![super::whole_object_borrow_conflict_message(
                whole_place,
                part_place,
                string_table,
            )]
        }
        DiagnosticPayload::InvalidMutableAccess {
            place,
            reason,
            conflicting_place,
        } => {
            vec![super::invalid_mutable_access_message(
                place,
                *reason,
                conflicting_place.as_ref(),
                string_table,
            )]
        }
        DiagnosticPayload::InvalidAccessAfterPossibleOwnershipTransfer { place } => {
            vec![
                super::invalid_access_after_possible_ownership_transfer_message(
                    place,
                    string_table,
                ),
            ]
        }
        DiagnosticPayload::UseOfUninitializedLocal { place } => {
            vec![super::use_of_uninitialized_local_message(
                place,
                string_table,
            )]
        }
        _ => Vec::new(),
    }
}
