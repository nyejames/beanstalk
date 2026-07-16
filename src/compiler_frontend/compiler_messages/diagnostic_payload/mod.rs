//! Typed diagnostic payload facts.
//!
//! WHAT: stores structured data needed to render or inspect diagnostics later.
//! WHY: compiler stages should carry stable IDs, source locations, and typed context rather than
//! pre-rendered strings or generic argument maps.

use crate::builder_surface::SourceFileKind;
use crate::compiler_frontend::compiler_messages::source_location::SourceLocation;
use crate::compiler_frontend::datatypes::ids::{GenericParameterId, TypeId};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap};
use crate::compiler_frontend::tokenizer::tokens::TokenKind;

mod remap;
mod types;

pub use types::*;

// --------------------------
//  Main Diagnostic Payloads
// --------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum DiagnosticPayload {
    None,

    // -----------------
    //  General Syntax
    // -----------------
    ExpectedToken {
        expected: TokenKind,
        found: Option<TokenKind>,
    },

    UnexpectedToken {
        found: TokenKind,
    },

    UnexpectedTrailingComma,

    UnescapedImplicitTemplateClose {
        source_kind: SourceFileKind,
    },

    UnknownName {
        name: StringId,
        namespace: NameNamespace,
    },

    TypeMismatch {
        expected: TypeId,
        found: TypeId,
        context: TypeMismatchContext,
    },

    DuplicateDeclaration {
        name: StringId,
        first_location: Option<SourceLocation>,
    },

    // -----------------
    //  Import payloads
    // -----------------
    MissingImportTarget {
        path: InternedPath,
    },

    AmbiguousImportTarget {
        path: InternedPath,
    },

    BareFileImport {
        path: InternedPath,
    },

    DirectSpecialFileImport {
        path: InternedPath,
    },

    ImportNameCollision {
        name: StringId,
        previous_location: Option<SourceLocation>,
    },

    NotExportedBySourceFile {
        symbol_path: InternedPath,
    },

    NotExportedByPublicSurface {
        requested_path: InternedPath,
        public_surface_name: StringId,
        public_surface_type: ImportPublicSurfaceType,
    },

    MissingModuleRootPublicSurface {
        symbol_path: InternedPath,
    },

    MissingPackageSymbol {
        symbol: StringId,
        package_path: StringId,
    },

    CrossModuleImportNotExported {
        symbol_path: InternedPath,
    },

    InvalidImportPath {
        path: InternedPath,
        reason: InvalidImportPathReason,
    },

    DirectSymbolPathImport {
        path: InternedPath,
    },

    InvalidNamespaceDefaultName {
        path: InternedPath,
    },

    DuplicateImportSurfaceMember {
        surface_path: InternedPath,
        member_name: StringId,
    },

    ExplicitBstExtension {
        path: InternedPath,
    },

    ExplicitSourceExtension {
        path: InternedPath,
        extension: StringId,
    },

    UnsupportedSourceFileKind {
        path: InternedPath,
        extension: StringId,
    },

    InvalidSourceFileEntry {
        path: InternedPath,
        extension: StringId,
    },

    InvalidBeandownApiScopeItem {
        path: InternedPath,
    },

    DuplicateBeandownInputPath {
        path: InternedPath,
        first_location: SourceLocation,
    },

    UnsupportedExternalExtension {
        path: InternedPath,
        extension: StringId,
    },

    InvalidExternalModule {
        path: InternedPath,
        message: StringId,
    },

    // -----------------
    //  Borrow Payloads
    // -----------------
    BorrowConflict {
        place: DiagnosticPlace,
        existing_access: BorrowAccessKind,
        requested_access: BorrowAccessKind,
    },

    MultipleMutableBorrows {
        place: DiagnosticPlace,
        existing_location: Option<SourceLocation>,
    },

    SharedMutableConflict {
        place: DiagnosticPlace,
        existing_access: BorrowAccessKind,
        requested_access: BorrowAccessKind,
        conflicting_place: Option<DiagnosticPlace>,
        existing_location: Option<SourceLocation>,
    },

    UseAfterPossibleMove {
        place: DiagnosticPlace,
        move_location: Option<SourceLocation>,
    },

    MoveWhileBorrowed {
        place: DiagnosticPlace,
        existing_access: BorrowAccessKind,
        borrow_location: Option<SourceLocation>,
    },

    WholeObjectBorrowConflict {
        whole_place: DiagnosticPlace,
        part_place: DiagnosticPlace,
        part_location: Option<SourceLocation>,
    },

    InvalidMutableAccess {
        place: DiagnosticPlace,
        reason: InvalidMutableAccessReason,
        conflicting_place: Option<DiagnosticPlace>,
    },

    InvalidAccessAfterPossibleOwnershipTransfer {
        place: DiagnosticPlace,
    },

    UseOfUninitializedLocal {
        place: DiagnosticPlace,
    },

    // -----------------
    //  Config Payloads
    // -----------------
    InvalidConfig {
        key: Option<StringId>,
        reason: InvalidConfigReason,
    },

    DeferredFeature {
        reason: DeferredFeatureReason,
    },

    UnsupportedExternalFunction {
        function_name: StringId,
        package_path: Option<StringId>,
        backend_name: StringId,
    },

    // ----------
    //  Warnings
    // ----------
    UnusedName {
        name: StringId,
    },

    UnreachableMatchArm,

    BstFilePathInTemplateOutput {
        path: StringId,
    },

    LargeTrackedAsset {
        path: StringId,
        byte_size: u64,
    },

    IdentifierNamingConvention {
        name: StringId,
        expected_style: NamingConvention,
    },

    ImportAliasCaseMismatch {
        alias: StringId,
        symbol: StringId,
    },

    MalformedTemplate {
        message: StringId,
    },

    // -----------------
    //  Syntax Payloads
    // -----------------
    InvalidCharacter {
        character: char,
    },

    InvalidStringEscape {
        reason: InvalidStringEscapeReason,
    },

    InvalidNumberLiteral {
        literal_text: StringId,
        reason: NumberLiteralErrorReason,
    },

    InvalidStyleDirective {
        directive_name: StringId,
        supported_directives: StringId,
    },

    MissingClosingDelimiter {
        expected_delimiter: StringId,
    },

    InvalidGenericApplication {
        reason: GenericApplicationErrorReason,
    },

    UnexpectedEndOfFile {
        expected_delimiter: Option<StringId>,
    },

    InvalidPath {
        path_kind: PathKind,
    },

    InvalidImportClause {
        clause_kind: ImportClauseKind,
        reason: InvalidImportClauseReason,
    },

    InvalidTypeAnnotation {
        context: TypeAnnotationContext,
        reason: InvalidTypeAnnotationReason,
    },

    InvalidCollectionType {
        reason: InvalidCollectionTypeReason,
    },

    InvalidMapType {
        reason: InvalidMapTypeReason,
    },

    InvalidMapLiteral {
        reason: InvalidMapLiteralReason,
    },

    InvalidGenericParameter {
        reason: InvalidGenericParameterReason,
    },

    InvalidTemplateDirective {
        directive_name: Option<StringId>,
        reason: InvalidTemplateDirectiveReason,
    },

    InvalidTemplateStructure {
        reason: InvalidTemplateStructureReason,
    },

    InvalidSignatureMember {
        reason: InvalidSignatureMemberReason,
    },

    InvalidFunctionSignature {
        reason: InvalidFunctionSignatureReason,
    },

    InvalidChoiceVariant {
        reason: InvalidChoiceVariantReason,
        choice_name: Option<StringId>,
        variant_name: Option<StringId>,
        available_variants: Vec<StringId>,
    },

    InvalidStructDefaultValue,

    UninitializedVariable {
        name: StringId,
    },

    CircularDependency {
        path: InternedPath,
    },

    NamespaceMisuse {
        name: StringId,
        expected: NameNamespace,
        found: NameNamespace,
    },

    ShadowedName {
        name: StringId,
        first_location: SourceLocation,
    },

    ReservedNameCollision {
        name: StringId,
        reserved_by: ReservedNameOwner,
    },

    InvalidThisUsage {
        reason: InvalidThisUsageReason,
    },

    InvalidReceiverDeclaration {
        reason: InvalidReceiverDeclarationReason,
    },

    InvalidControlFlowStatement {
        reason: InvalidControlFlowStatementReason,
    },

    InvalidDeclaration {
        reason: InvalidDeclarationReason,
        name: Option<StringId>,
    },

    InvalidAssignmentTarget {
        reason: InvalidAssignmentTargetReason,
        target_name: Option<StringId>,
        target_type: Option<TypeId>,
    },

    InvalidMultiBind {
        reason: InvalidMultiBindReason,
        target_name: Option<StringId>,
    },

    InvalidBuiltinCall {
        reason: InvalidBuiltinCallReason,
        builtin_name: Option<StringId>,
    },

    InvalidCast {
        reason: InvalidCastReason,
        source_type: Option<TypeId>,
        target_type: Option<TypeId>,
    },

    InvalidReceiverCall {
        reason: InvalidReceiverCallReason,
        receiver_type: Option<StringId>,
        method_name: Option<StringId>,
    },

    InvalidCopyTarget {
        reason: InvalidCopyTargetReason,
    },

    InvalidFieldAccess {
        reason: InvalidFieldAccessReason,
        field_name: Option<StringId>,
        receiver_type: Option<TypeId>,
        known_fields: Vec<StringId>,
    },

    InvalidMatchPattern {
        reason: InvalidMatchPatternReason,
        variant_name: Option<StringId>,
        scrutinee_name: Option<StringId>,
    },

    NonExhaustiveMatch {
        reason: NonExhaustiveMatchReason,
        missing_variants: Vec<StringId>,
    },

    InvalidResultHandling {
        reason: InvalidResultHandlingReason,
    },

    InvalidTemplateSlot {
        reason: InvalidTemplateSlotReason,
        slot_name: Option<StringId>,
    },

    CompileTimeEvaluationError {
        reason: CompileTimeEvaluationErrorReason,
        operation: Option<StringId>,
    },

    EmptyCollectionTypeAmbiguity,

    UnsupportedOperatorTypes {
        operator: DiagnosticOperator,
        lhs: TypeId,
        rhs: Option<TypeId>,
    },

    InvalidResultOperand {
        reason: InvalidResultOperandReason,
        category: UnsupportedOperatorCategory,
        operand_type: TypeId,
    },

    IncompatibleChoiceComparison {
        reason: IncompatibleChoiceComparisonReason,
        lhs: TypeId,
        rhs: TypeId,
    },

    InvalidCallShape {
        reason: InvalidCallShapeReason,
        callee_name: Option<StringId>,
    },

    InvalidReturnShape {
        reason: InvalidReturnShapeReason,
    },

    InvalidGenericInstantiation {
        type_name: Option<StringId>,
        reason: InvalidGenericInstantiationReason,
    },

    InvalidRangeOperand {
        operand: RangeOperandKind,
        found_type: TypeId,
    },

    UnsupportedBuilderPackage {
        package_path: StringId,
    },

    UnsupportedBackendFeature {
        backend_name: StringId,
        feature: StringId,
    },

    InvalidPageMetadata {
        key: StringId,
        reason: InvalidPageMetadataReason,
    },

    InvalidCompileTimePath {
        path: InternedPath,
        reason: InvalidCompileTimePathReason,
    },

    ImportRecordUsedAsValue {
        record_name: StringId,
    },

    ConstRecordUsedAsValue {
        record_name: StringId,
    },

    NestedTraversal {
        record_name: StringId,
    },

    NamespaceTypeValueMisuse {
        name: StringId,
        expected: NamespaceTypeValueMisuseKind,
        found: NamespaceTypeValueMisuseKind,
    },

    UnknownTrait {
        name: StringId,
    },

    DuplicateTraitRequirement {
        trait_name: StringId,
        requirement_name: StringId,
        first_location: SourceLocation,
    },

    TraitPrivateSurfaceLeak {
        trait_name: StringId,
        surface_type: TypeId,
    },

    GenericBoundPrivateSurfaceLeak {
        function_name: StringId,
        trait_name: StringId,
    },

    UnsupportedTraitFeature {
        trait_name: StringId,
        feature: StringId,
    },

    InvalidTraitKeywordUsage {
        reason: InvalidTraitKeywordUsageReason,
    },

    DuplicatePublicExport {
        name: StringId,
    },

    PrivateTypeInExportedApi {
        exported_name: StringId,
        private_type: TypeId,
    },

    InvalidTraitConformance {
        target_name: StringId,
        trait_name: Option<StringId>,
        reason: InvalidTraitConformanceReason,
    },

    InvalidTraitIncompatibility {
        subject_name: StringId,
        incompatible_trait_name: Option<StringId>,
        reason: InvalidTraitIncompatibilityReason,
    },

    TraitNameUsedAsType {
        trait_name: StringId,
    },

    InvalidExpression,

    MissingOperatorOperand {
        operator: StringId,
        position: OperatorOperandPosition,
    },

    InvalidStandaloneStatement {
        reason: InvalidStandaloneStatementReason,
    },

    ExpectedSymbolStatement,

    MissingCollectionItem,

    InvalidMatchArm {
        reason: InvalidMatchArmReason,
    },

    InvalidLoopHeader {
        reason: InvalidLoopHeaderReason,
    },

    InvalidStatementPosition {
        reason: InvalidStatementPositionReason,
    },

    CommonSyntaxMistake {
        reason: CommonSyntaxMistakeReason,
    },

    // -------------------------
    //  Infrastructure Payloads
    // -------------------------
    /// Boundary payload for direct internal/tooling `CompilerError` rendering.
    /// User-facing source diagnostics must use typed payload variants instead.
    InfrastructureError {
        msg: String,
        error_type: crate::compiler_frontend::compiler_errors::ErrorType,
        metadata: std::collections::HashMap<
            crate::compiler_frontend::compiler_errors::CompilerErrorMetadataKey,
            String,
        >,
    },
}
