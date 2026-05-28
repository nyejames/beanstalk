//! Compiler message models and render-boundary helpers.
//!
//! WHAT: owns typed user-facing diagnostics, internal/tooling error transport, source locations,
//! stage-local diagnostic bags, boundary aggregation, and final renderers.
//! WHY: compiler stages should exchange structured facts while CLI, dev-server, test, and tool
//! boundaries decide how those facts become user-visible text.
//!
//! `CompilerDiagnostic` is the normal source/config/import/type/rule/borrow diagnostic path.
//! `CompilerMessages` is the ordered boundary container that carries diagnostics with the
//! `StringTable` and optional type render context needed for prose. `CompilerError` is reserved
//! for internal compiler, filesystem, backend, and dev-server infrastructure failures.

pub(crate) mod compiler_dev_logging;
pub(crate) mod compiler_diagnostic;
pub(crate) mod compiler_errors;
pub(crate) mod diagnostic_bag;
pub(crate) mod diagnostic_descriptor;
pub(crate) mod diagnostic_kind;
mod diagnostic_kind_descriptors;
pub(crate) mod diagnostic_label;
pub(crate) mod diagnostic_payload;
pub(crate) mod diagnostic_severity;
pub(crate) mod display_messages;
pub(crate) mod render;
pub(crate) mod source_location;

pub(crate) use compiler_diagnostic::CompilerDiagnostic;
pub(crate) use diagnostic_bag::DiagnosticBag;
pub(crate) use diagnostic_descriptor::DiagnosticDescriptor;
pub(crate) use diagnostic_kind::{
    BorrowDiagnosticKind, ConfigDiagnosticKind, DeferredFeatureDiagnosticKind, DiagnosticCategory,
    DiagnosticKind, ImportDiagnosticKind, InfrastructureDiagnosticKind, RuleDiagnosticKind,
    SyntaxDiagnosticKind, TypeDiagnosticKind,
};
pub(crate) use diagnostic_label::{DiagnosticLabel, DiagnosticLabelMessage, DiagnosticLabelStyle};
pub(crate) use diagnostic_payload::{
    BorrowAccessKind, CommonSyntaxMistakeReason, CompileTimeEvaluationErrorReason,
    DeferredFeatureReason, DiagnosticPayload, DiagnosticPlace, GenericApplicationErrorReason,
    ImportClauseKind, ImportFacadeType, IncompatibleChoiceComparisonReason,
    InvalidAssignmentTargetReason, InvalidBuiltinCallReason, InvalidCallShapeReason,
    InvalidChoiceVariantReason, InvalidCollectionTypeReason, InvalidCompileTimePathReason,
    InvalidConfigReason, InvalidControlFlowStatementReason, InvalidCopyTargetReason,
    InvalidDeclarationReason, InvalidFieldAccessReason, InvalidFunctionSignatureReason,
    InvalidGenericInstantiationReason, InvalidGenericParameterReason, InvalidImportClauseReason,
    InvalidImportPathReason, InvalidLibraryFolderReason, InvalidLoopHeaderReason,
    InvalidMatchArmReason, InvalidMatchPatternReason, InvalidMultiBindReason,
    InvalidMutableAccessReason, InvalidPageMetadataReason, InvalidReceiverCallReason,
    InvalidReceiverDeclarationReason, InvalidResultHandlingReason, InvalidResultOperandReason,
    InvalidReturnShapeReason, InvalidSignatureMemberReason, InvalidStandaloneStatementReason,
    InvalidStatementPositionReason, InvalidTemplateDirectiveReason, InvalidTemplateSlotReason,
    InvalidTemplateStructureReason, InvalidThisUsageReason, InvalidTypeAnnotationReason,
    NameNamespace, NamespaceTypeValueMisuseKind, NamingConvention, NonExhaustiveMatchReason,
    NumberLiteralErrorReason, OperatorOperandPosition, PathKind, RangeOperandKind,
    ReservedNameOwner, TypeAnnotationContext, TypeMismatchContext, UnsupportedOperatorCategory,
};
pub(crate) use diagnostic_severity::DiagnosticSeverity;

#[cfg(test)]
#[path = "tests/diagnostic_model_tests.rs"]
mod diagnostic_model_tests;

#[cfg(test)]
#[path = "tests/type_rendering_tests.rs"]
mod type_rendering_tests;
