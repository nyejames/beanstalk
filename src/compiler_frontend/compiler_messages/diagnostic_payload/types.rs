//! Supporting diagnostic payload reason and context types.
//!
//! WHAT: stores the typed reason enums and small payload helper records used by
//! DiagnosticPayload variants.
//! WHY: separating these supporting facts keeps the top-level payload enum readable
//! while preserving structured diagnostics across compiler stages.

use super::*;

// -------------------------------
//  Diagnostic Payload Supporting Types
// -------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NameNamespace {
    Value,
    Type,
    Import,
    Module,
    Field,
    Variant,
    Function,
    Method,
    TemplateSlot,
    ConfigKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TypeMismatchContext {
    Assignment,
    Declaration,
    ReturnValue,
    FunctionArgument,
    ConstructorArgument,
    ReceiverArgument,
    Operator,
    Condition,
    CollectionElement,
    StructFieldDefault,
    TemplateInterpolation,
    MatchScrutinee,
    MatchPattern,
    ResultError,
    Pattern,
    General,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NamingConvention {
    CamelCase,
    LowercaseWithUnderscores,
    UppercaseWithUnderscores,
    LowercaseOrUppercaseWithUnderscores,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImportFacadeType {
    SourceLibrary,
    ModuleRoot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagnosticPlace {
    Local(StringId),
    Path(InternedPath),
    RenderedText(StringId),
    Unknown,
}

impl DiagnosticPlace {
    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            DiagnosticPlace::Local(name) | DiagnosticPlace::RenderedText(name) => {
                *name = remap.get(*name);
            }

            DiagnosticPlace::Path(path) => path.remap_string_ids(remap),

            DiagnosticPlace::Unknown => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BorrowAccessKind {
    Shared,
    Mutable,
    Move,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidMutableAccessReason {
    ImmutablePlace,
    OverlappingAccess,
    AliasedValueRequiresExclusiveAccess,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InvalidConfigReason {
    MissingKey,
    ShorthandDeclaration,
    DuplicateKey,
    DeprecatedSrcKey,
    ReplacedLibrariesKey,
    ReplacedRootFoldersKey,
    FunctionUnsupported,
    TraitDeclarationUnsupported,
    TraitConformanceUnsupported,
    TraitIncompatibilityUnsupported,
    MutableBindingUnsupported,
    PlainBindingUnsupported,
    UnsupportedStatement,
    StandaloneTemplateUnsupported,
    MissingValue,
    UnsupportedScalarValue,
    NotCompileTimeConstant,
    ValueCouldNotFold,
    UnsupportedLibraryFoldersValue,
    DuplicateLibraryFolder {
        folder: StringId,
    },
    InvalidLibraryFolder {
        folder: Option<StringId>,
        reason: InvalidLibraryFolderReason,
    },
    EmptyProjectSetting,
    UnknownKey {
        key: StringId,
    },
    InvalidConfigValueShape {
        expected: StringId,
    },
    InvalidProjectSettingValue {
        value: StringId,
        expected: StringId,
    },
    MissingHtmlHomepage {
        entry_root: StringId,
    },
    DuplicateHtmlOutputPath {
        output_path: StringId,
        entry_point: StringId,
        existing_entry_point: StringId,
    },
    TrackedAssetOutputConflict {
        asset_path: StringId,
        output_path: StringId,
        existing_owner: StringId,
    },
    TrackedAssetBuilderOutputConflict {
        asset_path: StringId,
        output_path: StringId,
    },
    ConfiguredEntryRootMissing {
        entry_root: StringId,
    },
    ConfiguredLibraryFolderMissing {
        folder: StringId,
    },
    ConfiguredLibraryFolderNotDirectory {
        folder: StringId,
    },
    SourceLibraryPrefixCollision {
        prefix: StringId,
        first_root: StringId,
        second_root: StringId,
    },
    SourceLibraryBuilderPrefixCollision {
        prefixes: StringId,
        library_folders: StringId,
    },
    EntryRootLibraryPrefixCollision {
        prefix: StringId,
        entry_folder: StringId,
    },
    SourceLibraryMissingRoot {
        prefix: StringId,
        root: StringId,
    },
    SourceLibraryMultipleRoots {
        prefix: StringId,
        root: StringId,
        candidates: Vec<StringId>,
    },
    NoRootModuleEntries {
        entry_root: StringId,
    },
    MultipleModuleRootFiles {
        directory: StringId,
        candidates: Vec<StringId>,
    },
    ConfigImportRootViolation,
    BstFileFolderCollision {
        file_name: StringId,
        folder_name: StringId,
        directory: StringId,
    },
}

impl InvalidConfigReason {
    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            Self::DuplicateLibraryFolder { folder } => {
                *folder = remap.get(*folder);
            }

            Self::InvalidLibraryFolder { folder, .. } => {
                if let Some(folder) = folder {
                    *folder = remap.get(*folder);
                }
            }

            Self::InvalidProjectSettingValue { value, expected } => {
                *value = remap.get(*value);
                *expected = remap.get(*expected);
            }

            Self::MissingHtmlHomepage { entry_root } => {
                *entry_root = remap.get(*entry_root);
            }

            Self::DuplicateHtmlOutputPath {
                output_path,
                entry_point,
                existing_entry_point,
            } => {
                *output_path = remap.get(*output_path);
                *entry_point = remap.get(*entry_point);
                *existing_entry_point = remap.get(*existing_entry_point);
            }

            Self::TrackedAssetOutputConflict {
                asset_path,
                output_path,
                existing_owner,
            } => {
                *asset_path = remap.get(*asset_path);
                *output_path = remap.get(*output_path);
                *existing_owner = remap.get(*existing_owner);
            }

            Self::TrackedAssetBuilderOutputConflict {
                asset_path,
                output_path,
            } => {
                *asset_path = remap.get(*asset_path);
                *output_path = remap.get(*output_path);
            }

            Self::ConfiguredEntryRootMissing { entry_root }
            | Self::NoRootModuleEntries { entry_root } => {
                *entry_root = remap.get(*entry_root);
            }

            Self::MultipleModuleRootFiles {
                directory,
                candidates,
            } => {
                *directory = remap.get(*directory);
                for candidate in candidates {
                    *candidate = remap.get(*candidate);
                }
            }

            Self::ConfiguredLibraryFolderMissing { folder }
            | Self::ConfiguredLibraryFolderNotDirectory { folder } => {
                *folder = remap.get(*folder);
            }

            Self::SourceLibraryPrefixCollision {
                prefix,
                first_root,
                second_root,
            } => {
                *prefix = remap.get(*prefix);
                *first_root = remap.get(*first_root);
                *second_root = remap.get(*second_root);
            }

            Self::SourceLibraryBuilderPrefixCollision {
                prefixes,
                library_folders,
            } => {
                *prefixes = remap.get(*prefixes);
                *library_folders = remap.get(*library_folders);
            }

            Self::EntryRootLibraryPrefixCollision {
                prefix,
                entry_folder,
            } => {
                *prefix = remap.get(*prefix);
                *entry_folder = remap.get(*entry_folder);
            }

            Self::SourceLibraryMissingRoot { prefix, root } => {
                *prefix = remap.get(*prefix);
                *root = remap.get(*root);
            }

            Self::SourceLibraryMultipleRoots {
                prefix,
                root,
                candidates,
            } => {
                *prefix = remap.get(*prefix);
                *root = remap.get(*root);
                for candidate in candidates {
                    *candidate = remap.get(*candidate);
                }
            }

            Self::BstFileFolderCollision {
                file_name,
                folder_name,
                directory,
            } => {
                *file_name = remap.get(*file_name);
                *folder_name = remap.get(*folder_name);
                *directory = remap.get(*directory);
            }

            Self::UnknownKey { key } => {
                *key = remap.get(*key);
            }

            Self::InvalidConfigValueShape { expected } => {
                *expected = remap.get(*expected);
            }

            Self::MissingKey
            | Self::ShorthandDeclaration
            | Self::DuplicateKey
            | Self::DeprecatedSrcKey
            | Self::ReplacedLibrariesKey
            | Self::ReplacedRootFoldersKey
            | Self::FunctionUnsupported
            | Self::TraitDeclarationUnsupported
            | Self::TraitConformanceUnsupported
            | Self::TraitIncompatibilityUnsupported
            | Self::MutableBindingUnsupported
            | Self::PlainBindingUnsupported
            | Self::UnsupportedStatement
            | Self::StandaloneTemplateUnsupported
            | Self::MissingValue
            | Self::UnsupportedScalarValue
            | Self::NotCompileTimeConstant
            | Self::ValueCouldNotFold
            | Self::UnsupportedLibraryFoldersValue
            | Self::EmptyProjectSetting
            | Self::ConfigImportRootViolation => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidLibraryFolderReason {
    Empty,
    AbsolutePath,
    ParentDirectorySegment,
    NestedPath,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumberLiteralErrorReason {
    SeparatorNotBetweenDigits,
    MultipleDecimalPoints,
    DecimalPointNotAfterDigit,
    EndsWithSeparator,
    MissingFractionalDigits,
    UppercaseExponentMarker,
    MissingExponentDigits,
    InvalidExponentSignPlacement,
    InvalidSeparatorPlacement,
    OutsideIntRange,
    NonFiniteFloat,
    ParseOverflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GenericApplicationErrorReason {
    OnNonNamedType,
    EmptyArgumentList,
    MissingArgumentAfterComma,
    NestedApplication,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PathKind {
    Empty,
    TrailingSeparator,
    InvalidRoot,
    InvalidComponent,
    InvalidGroupedSyntax,
    OnlyRootSlashSupported,
    SlashBeforeGroup,
    EmptyComponent,
    WhitespaceMustBeQuoted,
    MissingSeparator,
    MissingClosingBrace,
    MissingClosingQuote,
    InvalidEscape,
    EmptyGroupedBlock,
    EntriesNeedCommas,
    MultipleCommas,
    AliasOnlyOnLeaf,
    NestedGroupNeedsPrefix,
    GroupedEntryEmpty,
    GroupedPrefixTrailingSeparator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImportClauseKind {
    Import,
    Alias,
    Grouped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidImportClauseReason {
    MissingPath,
    ExpectedPath,
    MissingAlias,
    ExpectedAliasName,
    AliasNotValidIdentifier,
    AliasIsKeyword,
    GroupedWithTrailingAlias,
    PerEntryAndTrailingAlias,
    MultipleTrailingAliases,
    DoubleAliasInGroupedEntry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidImportPathReason {
    ParentDirectorySegment,
    EscapesProjectRoot,
    EscapesSourceLibraryRoot,
    CaseMismatch {
        provided: StringId,
        expected: StringId,
    },
}

impl InvalidImportPathReason {
    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            Self::CaseMismatch { provided, expected } => {
                *provided = remap.get(*provided);
                *expected = remap.get(*expected);
            }

            Self::ParentDirectorySegment
            | Self::EscapesProjectRoot
            | Self::EscapesSourceLibraryRoot => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidCompileTimePathReason {
    MissingTarget,
    EscapesProjectRoot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TypeAnnotationContext {
    DeclarationTarget,
    SignatureParameter,
    SignatureReturn,
    TypeAliasTarget,
    TraitRequirement,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InvalidTypeAnnotationReason {
    NoneNotAllowed,
    ReservedTraitKeyword,
    TraitThisMustBeDirect,
    AsNotValidHere,
    UnexpectedColon,
    ReactiveAccessNotAllowed,
    InvalidTokenAfterName { token: TokenKind },
    ExpectedTypeAnnotation { found: TokenKind },
    DuplicateOptional,
    NestedOptional,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidCollectionTypeReason {
    NegativeCapacity,
    /// Capacity-only shorthand `{N}` is not allowed in type signatures, aliases,
    /// struct fields, or return types. It is only valid for declaration targets
    /// where the initializer provides the element type.
    ShorthandCapacityNotAllowed,
    /// Fixed collection capacity must be greater than zero.
    ZeroCapacity,
    /// Capacity expression did not fold to an `Int` value.
    CapacityNotInt,
    /// Capacity expression references a non-constant value or contains runtime-only syntax.
    CapacityNotConstant,
    /// Folded capacity value does not fit in `usize`.
    CapacityOverflow,
    /// Collection literal has more items than the fixed collection capacity allows.
    InitializerExceedsFixedCapacity {
        capacity: usize,
        length: usize,
    },
    /// Immutable binding initialized with an empty fixed collection literal.
    EmptyImmutableFixedCollection,
    /// Capacity-only shorthand declaration requires a non-empty collection literal
    /// so the element type can be inferred.
    ShorthandEmptyLiteralAmbiguous,
    /// Capacity-only shorthand declaration requires a collection literal initializer.
    ShorthandNonLiteralRhs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidMapTypeReason {
    /// The key type is not one of the supported scalar types.
    UnsupportedKeyType { key_type: TypeId },
    /// Map types are nested too deeply inline; use a type alias instead.
    ExcessiveInlineNesting { depth: usize },
    /// Map type is missing the key type before the '=' separator.
    EmptyMapKeyType,
    /// Map type is missing the value type after the '=' separator.
    EmptyMapValueType,
    /// Map type contains more than one top-level '=' separator.
    MultipleMapSeparators,
    /// Fixed or capacity map syntax is outside the builtin hashmap design.
    FixedCapacityNotAllowed,
}

impl InvalidMapTypeReason {
    pub(crate) fn remap_string_ids(&mut self, _remap: &StringIdRemap) {
        match self {
            Self::UnsupportedKeyType { .. }
            | Self::ExcessiveInlineNesting { .. }
            | Self::EmptyMapKeyType
            | Self::EmptyMapValueType
            | Self::MultipleMapSeparators
            | Self::FixedCapacityNotAllowed => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidMapLiteralReason {
    /// A literal was classified as a map but an entry lacks a top-level `=`.
    MixedCollectionMapEntries,
    /// A foldable literal key appears more than once in the same map literal.
    DuplicateKnownKey,
    /// A map entry has `=` before any key expression.
    MissingKeyExpression,
    /// A map entry ends before a value expression appears after `=`.
    MissingValueExpression,
}

impl InvalidMapLiteralReason {
    pub(crate) fn remap_string_ids(&mut self, _remap: &StringIdRemap) {
        // All variants are unit-like; no string IDs to remap.
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum InvalidGenericParameterReason {
    EmptyParameterList,
    BoundsMustUseIs,
    ListMustStayWithHeader,
    InvalidToken { found: TokenKind },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidTemplateDirectiveReason {
    UnknownDirective,
    MissingArgument,
    InvalidArgument,
    DirectiveNotAllowedHere,
    /// Directive received parenthesized arguments it does not accept.
    UnexpectedArguments,
    /// Directive parentheses are empty.
    EmptyArguments,
    /// `$slot` received an invalid target argument.
    InvalidSlotTarget,
    /// `$insert` received an invalid slot name argument.
    InvalidInsertTarget,
    /// `$children` received an invalid wrapper argument.
    InvalidChildrenArgument,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidTemplateStructureReason {
    MissingClosingBracket,
    SlotInHead,
    MissingHandlerBody,
    InvalidChildDirective,
    NestedTemplateNotAllowed,
    HelperInConstTemplate,
    NonFoldableConstTemplate,
    NonFoldableDocComment,
    ResultInTemplateHead,
    UnsupportedTypeInTemplateHead { type_id: TypeId },
    RuntimeTemplateInConst,
    RuntimeValueInConstTemplateHead,
    ReactiveSubscriptionEmpty,
    ReactiveSubscriptionMultipleSources,
    ReactiveSubscriptionComplexExpression,
    ReactiveSubscriptionNonReactiveSource,
    ReactiveSubscriptionInConstTemplate,
    ReactiveSubscriptionOutsideTemplate,
    EmptyPathInTemplateHead,
    PathAliasInTemplateHead,
    IncompatibleHeadItem,
    HelperOutsideWrapperSlot,
    RuntimeControlFlowUnresolvedSlot,
    RuntimeControlFlowUnresolvedInsert,
    MissingCommaBeforeControlFlowSuffix,
    ControlFlowSuffixNotFinal,
    MissingTemplateIfCondition,
    MissingTemplateLoopHeader,
    ElseInTemplateHead,
    OrphanTemplateElse,
    OrphanTemplateElseIf,
    OrphanTemplateBreak,
    OrphanTemplateContinue,
    DuplicateTemplateElse,
    TemplateElseIfAfterElse,
    MalformedTemplateElse,
    MalformedTemplateElseIf,
    MalformedTemplateBreak,
    MalformedTemplateContinue,
    MissingTemplateElseIfCondition,
    InlineTemplateElse,
    InlineTemplateElseIf,
    InlineTemplateBreak,
    InlineTemplateContinue,
    TemplateElseInLiteralBody,
    TemplateElseIfInLiteralBody,
    TemplateLoopControlInLiteralBody,
    TemplateElseInLoopBody,
    TemplateElseIfInLoopBody,
    UnexpectedTokenAfterControlFlowSuffix,
    TemplateMatchStyleControlFlowUnsupported,
    TemplateIfConditionNotConst,
    TemplateIfBranchNotConst,
    TemplateOptionCaptureConstDeferred,
    TemplateLoopRangeBoundsNotConst,
    TemplateLoopSourceNotConst,
    TemplateLoopConditionNotConst,
    TemplateConditionalLoopConstTrue,
    TemplateLoopBodyNotConst,
    TemplateConstLoopExpansionLimitExceeded { limit: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidSignatureMemberReason {
    ChoicePayloadMutable,
    ChoicePayloadDefaultValue,
    CompileTimeParameterDeferred,
    ThisNotAllowed,
    TrailingComma,
    TraitReceiverMustBeThis,
    TraitMutableThisOnlyFirstParameter,
    TraitBareThisOnlyReceiver,
    TraitRequirementDefaultValue,
    ReactiveAccessNotAllowed,
    ReactiveParameterDefaultValue,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InvalidFunctionSignatureReason {
    MissingArrowOrColon { found: TokenKind },
    UnexpectedEndAfterParameters,
    UnexpectedColonAfterArrow,
    TrailingCommaInReturns,
    UnexpectedEndAfterComma,
    UnexpectedEndInReturns,
    MissingColonAfterReturns,
    UnexpectedArrowInReturns,
    MissingCommaOrColon { found: TokenKind },
    VoidNotAllowed,
    UnknownReturnAlias { name: StringId },
    MissingParameterNameInAlias,
    DuplicateParameterInAlias,
    AliasCannotBeError,
    AliasReturnNotAllowedInTraitRequirement,
    MultipleErrorReturnSlots,
    ErrorSlotNotLast,
    GenericWhereConstraintsUnsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidChoiceVariantReason {
    EmptyRecordBody,
    RecursiveDeclaration,
    ConstructorStyleNotSupported,
    PayloadShorthandNotSupported,
    UnexpectedSeparator,
    MissingVariants,
    UnknownVariant,
    UnitVariantWithParentheses,
    UnitVariantAsConstructor,
    PayloadVariantMissingArguments,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReservedNameOwner {
    BuiltinType,
    Keyword,
    CoreTrait,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidThisUsageReason {
    NotInReceiverMethod,
    Reassignment,
    LoopBinding,
    DeclarationBinding,
    DuplicateThis { function_name: StringId },
    NotFirstParameter { function_name: StringId },
    OutsideTraitDeclaration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidTraitKeywordUsageReason {
    MustOutsideTraitSyntax,
    ThisOutsideTraitSyntax,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidReceiverDeclarationReason {
    UnknownStructTarget,
    NonlocalSourceType,
    BuiltinScalarType,
    ExternalOpaqueType,
    FieldNameConflict,
    DuplicateMethod,
    DuplicateVisibleMethod,
    GenericReceiverType {
        function_name: StringId,
        type_name: StringId,
    },
    UnsupportedType {
        function_name: StringId,
        type_name: StringId,
    },
    ReceiverMethodImportNotAllowed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidControlFlowStatementReason {
    ElseOutsideIfOrMatch,
    ElseIfUnsupported,
    BreakOutsideLoop,
    ContinueOutsideLoop,
    TemplateInsideFunctionBody,
    ReturnOutsideFunction,
    ReturnBangOutsideErrorFunction,
    ExpectedColonAfterCondition,
    UnexpectedEndOfFileInMatch,
    CaseRequiredBeforeElse,
    DuplicateElseArm,
    ExpectedFatArrow,
    InlineValueIfMultiline,
    InlineValueIfElseThen,
    ValueIfMissingElse,
    ValueIfBranchFallsThrough,
    ValueIfNoProducingPath,
    ValueBlockOutsideReceiver,
    ValueIfOptionNonePredicate,
    ValueIfOptionLiteralPredicate,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InvalidDeclarationReason {
    ReservedBuiltinName,
    ConstantCannotBeMutable,
    ExternalTypeLiteralConstruction,
    ParameterizedGenericTypeAlias,
    UnusedGenericParameter { parameter_name: StringId },
    RecursiveGenericType,
    RecursiveRuntimeStruct { cycle: String },
    ExternalTypeAlias { type_name: StringId },
    InvalidGenericParameterName { parameter_name: StringId },
    DuplicateGenericParameter { parameter_name: StringId },
    GenericParameterNameCollision { parameter_name: StringId },
    ReservedGenericParameterName { parameter_name: StringId },
    GenericTraitsUnsupported,
    InvalidTraitName,
    TraitConformanceMissingTrait,
    TraitConformanceSemicolon,
    TraitIncompatibilityMissingTrait,
    TraitIncompatibilitySemicolon,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InvalidTraitIncompatibilityReason {
    SelfIncompatible,
    UnknownTrait,
    DuplicateRelation,
    PrivateTraitSurfaceLeak,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InvalidTraitConformanceReason {
    ModuleFacade,
    AliasTarget,
    NonCanonicalTarget,
    NonlocalSourceTarget,
    BuiltinTarget,
    ExternalOpaqueTarget,
    DuplicateCanonicalEvidence,
    BuiltinEvidenceOverride,
    IncompatibleTraitEvidence {
        incompatible_trait_name: StringId,
    },
    MissingMethod {
        requirement_name: StringId,
    },
    ReceiverMutabilityMismatch {
        requirement_name: StringId,
    },
    ParameterCountMismatch {
        requirement_name: StringId,
        expected: usize,
        found: usize,
    },
    ParameterModeMismatch {
        requirement_name: StringId,
        parameter_index: usize,
    },
    ParameterTypeMismatch {
        requirement_name: StringId,
        parameter_index: usize,
        expected_type: TypeId,
        found_type: TypeId,
    },
    ReturnCountMismatch {
        requirement_name: StringId,
        expected: usize,
        found: usize,
    },
    ReturnTypeMismatch {
        requirement_name: StringId,
        return_index: usize,
        expected_type: TypeId,
        found_type: TypeId,
    },
    ReturnChannelMismatch {
        requirement_name: StringId,
        return_index: usize,
    },
}

impl InvalidTraitConformanceReason {
    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            Self::MissingMethod { requirement_name }
            | Self::ReceiverMutabilityMismatch { requirement_name }
            | Self::ParameterCountMismatch {
                requirement_name, ..
            }
            | Self::ParameterModeMismatch {
                requirement_name, ..
            }
            | Self::ParameterTypeMismatch {
                requirement_name, ..
            }
            | Self::ReturnCountMismatch {
                requirement_name, ..
            }
            | Self::ReturnTypeMismatch {
                requirement_name, ..
            }
            | Self::ReturnChannelMismatch {
                requirement_name, ..
            } => {
                *requirement_name = remap.get(*requirement_name);
            }

            Self::ModuleFacade
            | Self::AliasTarget
            | Self::NonCanonicalTarget
            | Self::NonlocalSourceTarget
            | Self::BuiltinTarget
            | Self::ExternalOpaqueTarget
            | Self::DuplicateCanonicalEvidence
            | Self::BuiltinEvidenceOverride => {}

            Self::IncompatibleTraitEvidence {
                incompatible_trait_name,
            } => {
                *incompatible_trait_name = remap.get(*incompatible_trait_name);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidAssignmentTargetReason {
    NotMutablePlace,
    ImmutableVariable,
    UnavailableInCatchRecovery,
    CollectionIndexedWriteRemoved,
    MapIndexedWriteRemoved,
    MapPropertyWriteRemoved,
    ExpectedAssignmentOperator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidMultiBindReason {
    ThisTargetReserved,
    ExpectedTargetName,
    MissingTargetAfterComma,
    MissingAssignmentOperator,
    InvalidTokenAfterTarget,
    MissingRightHandExpression,
    MultipleRightHandExpressions,
    MutableTargetNeedsExplicitType,
    DuplicateTarget,
    UnsupportedRhs,
    ExistingTargetMutableMarker,
    ExistingTargetImmutable,
    ArityMismatch { expected: usize, found: usize },
    RhsNotMultiValue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidBuiltinCallReason {
    MissingParentheses,
    TakesNoArguments,
    NamedArgumentsNotSupported,
    MustHandleFallibleResult,
    DoesNotAcceptMutableAccess,
    CastMissingParentheses,
    CastMissingArgument,
    CastTooManyArguments,
    CastMissingClosingParenthesis,
    MissingArgument,
    TooManyArguments,
    RuntimeMessageExpressionDeferred,
    ExpressionPositionNotAllowed,
    MapLengthIsProperty,
    ScalarConstructorRemoved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidCastReason {
    MissingExplicitTarget,
    TargetNotBuiltin,
    TargetIsGenericParameter,
    SameSourceAndTarget,
    SourceIsOptional,
    OperandIsFallible,
    OperandArityMismatch,
    TargetArityMismatch,
    FallibleEvidenceRequiresHandling,
    InfallibleEvidenceCannotUseFallibleForm,
    PropagationRequiresErrorReturn,
    PropagationAndRecoveryConflict,
    BangMustAttachToCast,
    ScalarConstructorRemoved,
    NoEvidence,
    BuiltinEvidenceNotConstFoldable,
    UserDefinedEvidenceNotConstFoldable,
    GenericBoundEvidenceNotConstFoldable,
    BuiltinCastFailedInConst,
    CatchHandlerNotConstFoldable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidReceiverCallReason {
    CalledAsFreeFunction,
    MustUseParentheses,
    ConstStructNoRuntimeCalls,
    MutablePlaceRequired,
    MutableCollectionRequired,
    MutableMapRequired,
    MissingMutableAccessMarker,
    UnneededMutableAccessMarker,
    MutableMarkerOnNonReceiverCall,
    AmbiguousGenericBoundMethod,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidCopyTargetReason {
    FunctionValue,
    NonPlace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidFieldAccessReason {
    ExpectedNameAfterDot,
    FieldNotMethod,
    ChoicePayloadMutation,
    ChoicePayloadDeferred,
    UnknownExternalMember,
    UnknownMember,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidMatchPatternReason {
    WildcardNotSupported,
    AsNotValid,
    NegativeLiteralNotNumeric,
    LiteralTypeUnsupported,
    ScrutineeTypeUnsupportedForRelational,
    UnitVariantHasPayload,
    PayloadVariantNeedsBindings,
    CaptureBindingMustBeFieldName,
    ExpectedLocalBindingAfterAs,
    AliasMustBeLocalBinding,
    DuplicateCaptureBinding,
    TooManyCaptureBindings,
    CaptureBindingNameMismatch,
    TooFewCaptureBindings,
    QualifierDoesNotMatchScrutinee,
    ExpectedVariantNameAfterQualifier,
    MustUseVariantNamesNotLiterals,
    MustStartWithVariantName,
    UnknownVariant,
    CaptureBindingShadowsVariable,
    NonePatternRequiresOptionalScrutinee,
    OptionValuePatternRequiresEquality,
    BareCaptureOnOptionalScrutinee,
    OptionPresentCaptureOnNonOptional,
    EmptyOptionPresentCapture,
    OptionPresentCaptureTypeAnnotation,
    MissingClosingPipe,
    ExpectedBindingInOptionPresentCapture,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NonExhaustiveMatchReason {
    MissingElseArm,
    MissingVariants,
    GuardedArmsRequireElse,
    MissingOptionPatterns,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidResultHandlingReason {
    CatchOutsideBoundary,
    ExpectedCatchBlockOrHandler,
    ExpectedCatchHandlerOpeningPipe,
    ExpectedCatchHandlerIdentifier,
    ExpectedCatchHandlerClosingPipe,
    ExpectedCatchHandlerColon,
    EmptyCatchHandlerBinding,
    MultipleCatchHandlerBindings,
    RemovedBangFallbackSyntax,
    RemovedBangCatchHandlerSyntax,
    FallbackValuesForErrorOnlyResult,
    NotResultExpression,
    FunctionHasNoErrorSlot,
    NotOptionExpression,
    FunctionHasNoOptionalReturn,
    OptionPropagationReturnTypeMismatch,
    OptionPropagationCatchConflict,
    CatchHandlerConflicts,
    CatchHandlerCanFallThrough,
    InlineCatchMultiline,
    ThenWithNoActiveValueTarget,
    ThenCrossesBlockedConstruct,
    ThenRequiresValues,
    DirectOptionFallbackSyntax,
    UnhandledErrorReturn,
    SuccessValueDiscarded,
}

impl InvalidResultHandlingReason {
    pub(crate) fn message(self) -> &'static str {
        match self {
            InvalidResultHandlingReason::CatchOutsideBoundary => {
                "`catch` can only handle a fallible expression at an assignment, declaration, return, or statement boundary."
            }

            InvalidResultHandlingReason::ExpectedCatchBlockOrHandler => {
                "Expected `:` or `|err|:` catch-block syntax after `catch`."
            }

            InvalidResultHandlingReason::ExpectedCatchHandlerOpeningPipe => {
                "Expected `|` to start the catch handler binding."
            }

            InvalidResultHandlingReason::ExpectedCatchHandlerIdentifier => {
                "Expected a catch handler identifier between `|` markers."
            }

            InvalidResultHandlingReason::ExpectedCatchHandlerClosingPipe => {
                "Expected `|` after the catch handler identifier."
            }

            InvalidResultHandlingReason::ExpectedCatchHandlerColon => {
                "Expected ':' to start the catch handler scope."
            }

            InvalidResultHandlingReason::EmptyCatchHandlerBinding => {
                "`catch ||:` is invalid. Use `catch:` when the error value is unused."
            }

            InvalidResultHandlingReason::MultipleCatchHandlerBindings => {
                "`catch` accepts one optional error binding. Use `catch |err|:`."
            }

            InvalidResultHandlingReason::RemovedBangFallbackSyntax => {
                "Fallible fallbacks use `catch:` with `then` recovery values, not `! fallback`."
            }

            InvalidResultHandlingReason::RemovedBangCatchHandlerSyntax => {
                "Catch handlers use `catch |err|:`, not `err!`."
            }

            InvalidResultHandlingReason::FallbackValuesForErrorOnlyResult => {
                "This result has no success return values, so handler fallback values are not allowed here."
            }

            InvalidResultHandlingReason::NotResultExpression => {
                "The '!' result-handling suffix is only valid for Result-valued expressions."
            }

            InvalidResultHandlingReason::FunctionHasNoErrorSlot => {
                "This expression uses '!' propagation, but the surrounding function does not declare an error return slot."
            }

            InvalidResultHandlingReason::NotOptionExpression => {
                "The '?' option-propagation suffix is only valid for optional expressions."
            }

            InvalidResultHandlingReason::FunctionHasNoOptionalReturn => {
                "This expression uses '?' propagation, but the surrounding function does not return an optional success value."
            }

            InvalidResultHandlingReason::OptionPropagationReturnTypeMismatch => {
                "This '?' propagation expression is not compatible with the surrounding function's optional return type."
            }

            InvalidResultHandlingReason::OptionPropagationCatchConflict => {
                "`catch` handles fallible results. Optional values must use explicit option inspection instead of `? catch`."
            }

            InvalidResultHandlingReason::CatchHandlerConflicts => {
                "Catch handler conflicts with an existing visible declaration."
            }

            InvalidResultHandlingReason::CatchHandlerCanFallThrough => {
                "Catch handler without fallback can fall through while success values are required."
            }

            InvalidResultHandlingReason::InlineCatchMultiline => {
                "Inline `catch then` recovery must fit on a single logical line."
            }

            InvalidResultHandlingReason::ThenWithNoActiveValueTarget => {
                "`then` is only valid inside a value-producing block."
            }

            InvalidResultHandlingReason::ThenCrossesBlockedConstruct => {
                "`then` cannot target a value-producing block across this construct."
            }

            InvalidResultHandlingReason::ThenRequiresValues => {
                "`then` must produce at least one value."
            }

            InvalidResultHandlingReason::DirectOptionFallbackSyntax => {
                "Optional values do not support direct fallback syntax. Use `if option is |value| ... else ...`."
            }

            InvalidResultHandlingReason::UnhandledErrorReturn => {
                "Calls to error-returning functions must be explicitly handled with postfix `!` or `catch`."
            }

            InvalidResultHandlingReason::SuccessValueDiscarded => {
                "This fallible expression returns success values, so it cannot be used as a standalone statement."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidTemplateSlotReason {
    InsertOutsideParentSlot,
    ExtraLooseContentWithoutDefaultSlot,
    LooseContentWithoutDefaultSlot,
    InsertCannotTargetDefaultSlot,
    InsertTargetsUnknownNamedSlot,
    InsertTargetsUnknownPositionalSlot,
    MultipleDefaultSlots,
    SlotDefinitionOutsideTemplateBody,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompileTimeEvaluationErrorReason {
    IntegerOverflow,
    FloatOverflow,
    DivideByZero,
    InvalidExponent,
    InvalidOperatorForType,
    IntegerDivisionOnlyIntInt,
    ConstantSelfReference,
    ConstantNotVisible,
    NonConstantReferenceInConstant,
    SameFileForwardConstantReference,
    ConstantInitializerNotFoldable,
    ExternalNonScalarConstantInConstantContext,
    ExternalFunctionCallInConstantContext,
    NonCompileTimeFieldInConstantContext,
    NoneLiteralRequiresOptionalTypeContext,
    ExternalTypeConstructionNotSupported,
    StructFieldDefaultNotFoldable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnsupportedOperatorCategory {
    Arithmetic,
    Comparison,
    Range,
    Logical,
    Unary,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidResultOperandReason {
    ResultNotUnwrapped,
    OptionNotUnwrapped,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum IncompatibleChoiceComparisonReason {
    DifferentChoiceTypes,
    ChoiceWithNonChoice,
    PayloadEqualityNotSupported {
        field_name: StringId,
        field_type: TypeId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InvalidCallShapeReason {
    MissingArgument {
        parameter_name: Option<StringId>,
        parameter_index: usize,
    },

    ExtraPositionalArgument {
        expected_count: usize,
    },

    DuplicateArgument {
        parameter_name: Option<StringId>,
        parameter_index: usize,
    },

    NamedArgumentNotFound {
        name: StringId,
        known_parameters: Vec<StringId>,
    },

    PositionalAfterNamed,

    NamedArgumentsNotSupported,

    MutableAccessRequired {
        parameter_name: Option<StringId>,
        parameter_index: usize,
    },

    MutableAccessNotAllowed {
        parameter_name: Option<StringId>,
        parameter_index: usize,
    },

    MutableAccessOnNonPlace {
        parameter_name: Option<StringId>,
        parameter_index: usize,
    },

    MutableAccessOnImmutablePlace {
        parameter_name: Option<StringId>,
        parameter_index: usize,
    },

    ReactiveSourceRequired {
        parameter_name: Option<StringId>,
        parameter_index: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidReturnShapeReason {
    BareReturnWithExpectedValues {
        expected_count: usize,
    },

    ReturnValuesWithBareSignature,

    TooManyReturnValues {
        expected_count: usize,
    },

    TooFewReturnValues {
        expected_count: usize,
        provided_count: usize,
    },

    MissingReturnBangValue,

    FunctionMayFallThrough,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DeferredFeatureReason {
    NamedFeature { feature: StringId },
    CaptureTaggedPattern,
    NegatedMatchPattern,
    NamedPayloadPatternAssignment,
    NestedPayloadPattern,
    ChoiceVariantDefaultValue,
    GenericReceiverMethod,
    PublicOptionTypeSyntax,
    PublicResultTypeSyntax,
    CheckedBlock,
    AsyncBlock,
}

impl DeferredFeatureReason {
    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        if let Self::NamedFeature { feature } = self {
            *feature = remap.get(*feature);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidPageMetadataReason {
    NotAString,
    DuplicateDeclaration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RangeOperandKind {
    Start,
    End,
    Step,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OperatorOperandPosition {
    Unary,
    BinaryLeft,
    BinaryRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidStandaloneStatementReason {
    FieldRead,
    Expression,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NamespaceTypeValueMisuseKind {
    Namespace,
    Type,
    Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidMatchArmReason {
    SemicolonDelimiter,
    LegacyColonSyntax,
    LegacyElseSyntax,
    InvalidArrow,
    ArmMustStartNewLine,
    ExpectedArmHeader,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidLoopHeaderReason {
    EmptyHeader,
    MissingColon,
    RemovedInSyntax,
    MissingClosingPipe,
    MalformedBindingPipes,
    MissingSourceBeforeBindings,
    EmptyBindingList,
    ThisBinding,
    BindingMustBeSymbol,
    MissingBindingComma,
    TrailingBindingComma,
    BareSingleBinding,
    BareDualBinding,
    TooManyBindings,
    DuplicateBindingName,
    BindingAlreadyDeclared,
    CollectionSourceNotCollection { found_type: TypeId },
    MissingRangeSeparator,
    MissingRangeEndBound,
    MissingRangeStep,
    FloatRangeMissingStep,
    ZeroRangeStep,
    ExpectedHeaderExpression,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvalidStatementPositionReason {
    UnexpectedComma,
    UnexpectedCloseParenthesis,
    UnexpectedCloseCurly,
    UnexpectedPipe,
    UnexpectedArrow,
    UnexpectedWildcard,
    ReservedGenericDeclaration,
    UnexpectedOf,
    UnexpectedScopeCloseInExpression,
    UnexpectedScopeCloseInTemplate,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CommonSyntaxMistakeReason {
    EqualityOperator,
    InequalityOperator,
    LogicalAndOperator,
    LogicalOrOperator,
    BooleanBangNegation,
    ExpressionAssignment,
    RustBorrowPrefix,
    InvalidAsOperator,
    StatementLineComment,
    FunctionKeyword { keyword: StringId },
    LetOrVarKeyword,
    ConstKeyword,
    MatchKeyword,
    StructKeyword { keyword: StringId },
    SignatureParenthesisDelimiter,
    SignatureAsKeyword,
    InvalidCompileTimeBindingSpacing,
    InvalidMutableBindingSpacing,
    InvalidReactiveBindingSpacing,
    InvalidSymbolicBinaryOperatorSpacing,
    InvalidUnaryNegationSpacing,
    UnsupportedUnaryPlus,
}

impl CommonSyntaxMistakeReason {
    pub(super) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            CommonSyntaxMistakeReason::FunctionKeyword { keyword }
            | CommonSyntaxMistakeReason::StructKeyword { keyword } => {
                *keyword = remap.get(*keyword);
            }

            CommonSyntaxMistakeReason::EqualityOperator
            | CommonSyntaxMistakeReason::InequalityOperator
            | CommonSyntaxMistakeReason::LogicalAndOperator
            | CommonSyntaxMistakeReason::LogicalOrOperator
            | CommonSyntaxMistakeReason::BooleanBangNegation
            | CommonSyntaxMistakeReason::ExpressionAssignment
            | CommonSyntaxMistakeReason::RustBorrowPrefix
            | CommonSyntaxMistakeReason::InvalidAsOperator
            | CommonSyntaxMistakeReason::StatementLineComment
            | CommonSyntaxMistakeReason::LetOrVarKeyword
            | CommonSyntaxMistakeReason::ConstKeyword
            | CommonSyntaxMistakeReason::MatchKeyword
            | CommonSyntaxMistakeReason::SignatureParenthesisDelimiter
            | CommonSyntaxMistakeReason::SignatureAsKeyword
            | CommonSyntaxMistakeReason::InvalidCompileTimeBindingSpacing
            | CommonSyntaxMistakeReason::InvalidMutableBindingSpacing
            | CommonSyntaxMistakeReason::InvalidReactiveBindingSpacing
            | CommonSyntaxMistakeReason::InvalidSymbolicBinaryOperatorSpacing
            | CommonSyntaxMistakeReason::InvalidUnaryNegationSpacing
            | CommonSyntaxMistakeReason::UnsupportedUnaryPlus => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InvalidGenericInstantiationReason {
    WrongArgumentCount {
        expected: usize,
        found: usize,
    },
    TypeDoesNotAcceptArguments,
    ExternalTypeArgumentsUnsupported,
    MissingTypeArguments,
    CannotInferArguments {
        missing_parameters: Vec<StringId>,
    },
    CannotInferFunctionArguments {
        missing_parameters: Vec<StringId>,
    },
    ConflictingFunctionArgument {
        parameter_id: GenericParameterId,
        parameter_name: StringId,
        existing_type_id: TypeId,
        replacement_type_id: TypeId,
        current_evidence_location: SourceLocation,
        previous_evidence_location: Option<SourceLocation>,
    },
    MissingTraitEvidence {
        parameter_name: StringId,
        trait_name: StringId,
        concrete_type_id: TypeId,
    },
    MissingNominalTraitEvidence {
        parameter_name: StringId,
        trait_name: StringId,
        concrete_type_id: TypeId,
    },
    RecursiveFunctionInstantiation,
    ExplicitCallTypeArgumentsUnsupported,
    GenericFunctionValueDeferred,
}
