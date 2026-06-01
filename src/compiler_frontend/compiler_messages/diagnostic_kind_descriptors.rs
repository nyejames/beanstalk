//! Stable descriptor table for diagnostic kinds.
//!
//! WHAT: maps each diagnostic kind to the code, title, and default severity exposed at render
//! boundaries.
//! WHY: keeping the large mapping out of the taxonomy file makes the enum definitions easier to
//! scan while preserving one authoritative descriptor source.

use super::diagnostic_kind::{
    BorrowDiagnosticKind, ConfigDiagnosticKind, DeferredFeatureDiagnosticKind, DiagnosticKind,
    ImportDiagnosticKind, InfrastructureDiagnosticKind, RuleDiagnosticKind, SyntaxDiagnosticKind,
    TypeDiagnosticKind,
};
use crate::compiler_frontend::compiler_messages::{DiagnosticDescriptor, DiagnosticSeverity};

pub(super) fn descriptor_for_kind(kind: DiagnosticKind) -> DiagnosticDescriptor {
    match kind {
        DiagnosticKind::Syntax(kind) => syntax_descriptor(kind),
        DiagnosticKind::Type(kind) => type_descriptor(kind),
        DiagnosticKind::Rule(kind) => rule_descriptor(kind),
        DiagnosticKind::Import(kind) => import_descriptor(kind),
        DiagnosticKind::Borrow(kind) => borrow_descriptor(kind),
        DiagnosticKind::Config(kind) => config_descriptor(kind),
        DiagnosticKind::Infrastructure(kind) => infrastructure_descriptor(kind),
        DiagnosticKind::DeferredFeature(kind) => deferred_feature_descriptor(kind),
    }
}

fn syntax_descriptor(kind: SyntaxDiagnosticKind) -> DiagnosticDescriptor {
    match kind {
        SyntaxDiagnosticKind::ExpectedToken => DiagnosticDescriptor::new(
            "BST-SYNTAX-0001",
            "Expected token",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::UnexpectedToken => DiagnosticDescriptor::new(
            "BST-SYNTAX-0002",
            "Unexpected token",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::UnexpectedTrailingComma => DiagnosticDescriptor::new(
            "BST-SYNTAX-0003",
            "Unexpected trailing comma",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::MalformedCssTemplate => DiagnosticDescriptor::new(
            "BST-SYNTAX-0004",
            "Malformed CSS template",
            DiagnosticSeverity::Warning,
        ),
        SyntaxDiagnosticKind::MalformedHtmlTemplate => DiagnosticDescriptor::new(
            "BST-SYNTAX-0005",
            "Malformed HTML template",
            DiagnosticSeverity::Warning,
        ),
        SyntaxDiagnosticKind::UnterminatedStringLiteral => DiagnosticDescriptor::new(
            "BST-SYNTAX-0006",
            "Unterminated string literal",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::InvalidCharacter => DiagnosticDescriptor::new(
            "BST-SYNTAX-0007",
            "Invalid character",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::InvalidNumberLiteral => DiagnosticDescriptor::new(
            "BST-SYNTAX-0008",
            "Invalid number literal",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::InvalidCharLiteral => DiagnosticDescriptor::new(
            "BST-SYNTAX-0009",
            "Invalid character literal",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::InvalidStyleDirective => DiagnosticDescriptor::new(
            "BST-SYNTAX-0010",
            "Invalid style directive",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::InvalidIdentifier => DiagnosticDescriptor::new(
            "BST-SYNTAX-0011",
            "Invalid identifier",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::MissingClosingDelimiter => DiagnosticDescriptor::new(
            "BST-SYNTAX-0012",
            "Missing closing delimiter",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::UnexpectedTokenInDeclaration => DiagnosticDescriptor::new(
            "BST-SYNTAX-0013",
            "Unexpected token in declaration",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::InvalidTypeAnnotation => DiagnosticDescriptor::new(
            "BST-SYNTAX-0014",
            "Invalid type annotation",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::InvalidGenericApplication => DiagnosticDescriptor::new(
            "BST-SYNTAX-0015",
            "Invalid generic application",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::InvalidCollectionType => DiagnosticDescriptor::new(
            "BST-SYNTAX-0016",
            "Invalid collection type",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::UnexpectedEndOfFile => DiagnosticDescriptor::new(
            "BST-SYNTAX-0017",
            "Unexpected end of file",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::InvalidPath => {
            DiagnosticDescriptor::new("BST-SYNTAX-0018", "Invalid path", DiagnosticSeverity::Error)
        }
        SyntaxDiagnosticKind::InvalidImportClause => DiagnosticDescriptor::new(
            "BST-SYNTAX-0019",
            "Invalid import clause",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::InvalidGenericParameter => DiagnosticDescriptor::new(
            "BST-SYNTAX-0020",
            "Invalid generic parameter",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::InvalidTemplateDirective => DiagnosticDescriptor::new(
            "BST-SYNTAX-0021",
            "Invalid template directive",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::InvalidTemplateStructure => DiagnosticDescriptor::new(
            "BST-SYNTAX-0022",
            "Invalid template structure",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::InvalidExpression => DiagnosticDescriptor::new(
            "BST-SYNTAX-0023",
            "Invalid expression",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::MissingOperatorOperand => DiagnosticDescriptor::new(
            "BST-SYNTAX-0024",
            "Missing operator operand",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::InvalidStandaloneStatement => DiagnosticDescriptor::new(
            "BST-SYNTAX-0025",
            "Invalid standalone statement",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::ExpectedSymbolStatement => DiagnosticDescriptor::new(
            "BST-SYNTAX-0026",
            "Expected symbol statement",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::MissingCollectionItem => DiagnosticDescriptor::new(
            "BST-SYNTAX-0027",
            "Missing collection item",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::InvalidMatchArm => DiagnosticDescriptor::new(
            "BST-SYNTAX-0028",
            "Invalid match arm",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::InvalidLoopHeader => DiagnosticDescriptor::new(
            "BST-SYNTAX-0029",
            "Invalid loop header",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::InvalidStatementPosition => DiagnosticDescriptor::new(
            "BST-SYNTAX-0030",
            "Invalid statement position",
            DiagnosticSeverity::Error,
        ),
        SyntaxDiagnosticKind::CommonSyntaxMistake => DiagnosticDescriptor::new(
            "BST-SYNTAX-0031",
            "Common syntax mistake",
            DiagnosticSeverity::Error,
        ),
    }
}

fn type_descriptor(kind: TypeDiagnosticKind) -> DiagnosticDescriptor {
    match kind {
        TypeDiagnosticKind::TypeMismatch => {
            DiagnosticDescriptor::new("BST-TYPE-0001", "Type mismatch", DiagnosticSeverity::Error)
        }
        TypeDiagnosticKind::EmptyCollectionTypeAmbiguity => DiagnosticDescriptor::new(
            "BST-TYPE-0002",
            "Empty collection type ambiguity",
            DiagnosticSeverity::Error,
        ),
        TypeDiagnosticKind::UnsupportedOperatorTypes => DiagnosticDescriptor::new(
            "BST-TYPE-0003",
            "Unsupported operator types",
            DiagnosticSeverity::Error,
        ),
        TypeDiagnosticKind::InvalidResultOperand => DiagnosticDescriptor::new(
            "BST-TYPE-0004",
            "Invalid result operand",
            DiagnosticSeverity::Error,
        ),
        TypeDiagnosticKind::IncompatibleChoiceComparison => DiagnosticDescriptor::new(
            "BST-TYPE-0005",
            "Incompatible choice comparison",
            DiagnosticSeverity::Error,
        ),
    }
}

fn rule_descriptor(kind: RuleDiagnosticKind) -> DiagnosticDescriptor {
    match kind {
        RuleDiagnosticKind::UnknownName => {
            DiagnosticDescriptor::new("BST-RULE-0001", "Unknown name", DiagnosticSeverity::Error)
        }
        RuleDiagnosticKind::DuplicateDeclaration => DiagnosticDescriptor::new(
            "BST-RULE-0002",
            "Duplicate declaration",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::UnusedVariable => DiagnosticDescriptor::new(
            "BST-RULE-0010",
            "Unused variable",
            DiagnosticSeverity::Warning,
        ),
        RuleDiagnosticKind::UnusedFunction => DiagnosticDescriptor::new(
            "BST-RULE-0011",
            "Unused function",
            DiagnosticSeverity::Warning,
        ),
        RuleDiagnosticKind::UnusedType => {
            DiagnosticDescriptor::new("BST-RULE-0012", "Unused type", DiagnosticSeverity::Warning)
        }
        RuleDiagnosticKind::UnusedConstant => DiagnosticDescriptor::new(
            "BST-RULE-0013",
            "Unused constant",
            DiagnosticSeverity::Warning,
        ),
        RuleDiagnosticKind::UnusedFunctionArgument => DiagnosticDescriptor::new(
            "BST-RULE-0014",
            "Unused function argument",
            DiagnosticSeverity::Warning,
        ),
        RuleDiagnosticKind::UnusedFunctionReturnValue => DiagnosticDescriptor::new(
            "BST-RULE-0015",
            "Unused function return value",
            DiagnosticSeverity::Warning,
        ),
        RuleDiagnosticKind::UnusedFunctionParameter => DiagnosticDescriptor::new(
            "BST-RULE-0016",
            "Unused function parameter",
            DiagnosticSeverity::Warning,
        ),
        RuleDiagnosticKind::UnusedFunctionParameterDefaultValue => DiagnosticDescriptor::new(
            "BST-RULE-0017",
            "Unused function parameter default value",
            DiagnosticSeverity::Warning,
        ),
        RuleDiagnosticKind::BstFilePathInTemplateOutput => DiagnosticDescriptor::new(
            "BST-RULE-0019",
            "Beanstalk source path in template output",
            DiagnosticSeverity::Warning,
        ),
        RuleDiagnosticKind::LargeTrackedAsset => DiagnosticDescriptor::new(
            "BST-RULE-0020",
            "Large tracked asset",
            DiagnosticSeverity::Warning,
        ),
        RuleDiagnosticKind::IdentifierNamingConvention => DiagnosticDescriptor::new(
            "BST-RULE-0021",
            "Identifier naming convention",
            DiagnosticSeverity::Warning,
        ),
        RuleDiagnosticKind::UnreachableMatchArm => DiagnosticDescriptor::new(
            "BST-RULE-0022",
            "Unreachable match arm",
            DiagnosticSeverity::Warning,
        ),
        RuleDiagnosticKind::InvalidTopLevelRuntimeStatement => DiagnosticDescriptor::new(
            "BST-RULE-0023",
            "Invalid top-level runtime statement",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::LegacyImportSyntax => DiagnosticDescriptor::new(
            "BST-RULE-0025",
            "Legacy `#import` syntax is no longer supported",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::OldPrefixDeclarationSyntax => DiagnosticDescriptor::new(
            "BST-RULE-0064",
            "`#` is no longer a declaration prefix",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::RuntimeTemplateInModuleFacade => DiagnosticDescriptor::new(
            "BST-RULE-0026",
            "Runtime template in module facade",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::ReservedBuiltinName => DiagnosticDescriptor::new(
            "BST-RULE-0027",
            "Reserved builtin name",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidSignatureMember => DiagnosticDescriptor::new(
            "BST-RULE-0028",
            "Invalid signature member",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidChoiceVariant => DiagnosticDescriptor::new(
            "BST-RULE-0029",
            "Invalid choice variant",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidStructDefaultValue => DiagnosticDescriptor::new(
            "BST-RULE-0030",
            "Invalid struct default value",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::UninitializedVariable => DiagnosticDescriptor::new(
            "BST-RULE-0031",
            "Uninitialized variable",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::MissingImportTarget => DiagnosticDescriptor::new(
            "BST-RULE-0032",
            "Missing import target",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::CircularDependency => DiagnosticDescriptor::new(
            "BST-RULE-0033",
            "Circular dependency",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::UnknownValueName => DiagnosticDescriptor::new(
            "BST-RULE-0034",
            "Unknown value name",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::UnknownTypeName => DiagnosticDescriptor::new(
            "BST-RULE-0035",
            "Unknown type name",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::ValueUsedAsType => DiagnosticDescriptor::new(
            "BST-RULE-0036",
            "Value used as type",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::TypeUsedAsValue => DiagnosticDescriptor::new(
            "BST-RULE-0037",
            "Type used as value",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::ShadowedName => {
            DiagnosticDescriptor::new("BST-RULE-0038", "Shadowed name", DiagnosticSeverity::Error)
        }
        RuleDiagnosticKind::ReservedNameCollision => DiagnosticDescriptor::new(
            "BST-RULE-0039",
            "Reserved name collision",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidThisUsage => DiagnosticDescriptor::new(
            "BST-RULE-0040",
            "Invalid this usage",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidReceiverDeclaration => DiagnosticDescriptor::new(
            "BST-RULE-0041",
            "Invalid receiver declaration",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidControlFlowStatement => DiagnosticDescriptor::new(
            "BST-RULE-0042",
            "Invalid control flow statement",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidDeclaration => DiagnosticDescriptor::new(
            "BST-RULE-0043",
            "Invalid declaration",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidAssignmentTarget => DiagnosticDescriptor::new(
            "BST-RULE-0044",
            "Invalid assignment target",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidMultiBind => DiagnosticDescriptor::new(
            "BST-RULE-0045",
            "Invalid multi-bind",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidBuiltinCall => DiagnosticDescriptor::new(
            "BST-RULE-0046",
            "Invalid builtin call",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidReceiverCall => DiagnosticDescriptor::new(
            "BST-RULE-0047",
            "Invalid receiver call",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidCopyTarget => DiagnosticDescriptor::new(
            "BST-RULE-0056",
            "Invalid copy target",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidFieldAccess => DiagnosticDescriptor::new(
            "BST-RULE-0048",
            "Invalid field access",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidMatchPattern => DiagnosticDescriptor::new(
            "BST-RULE-0049",
            "Invalid match pattern",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::NonExhaustiveMatch => DiagnosticDescriptor::new(
            "BST-RULE-0050",
            "Non-exhaustive match",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidResultHandling => DiagnosticDescriptor::new(
            "BST-RULE-0051",
            "Invalid result handling",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidTemplateSlot => DiagnosticDescriptor::new(
            "BST-RULE-0052",
            "Invalid template slot",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::CompileTimeEvaluationError => DiagnosticDescriptor::new(
            "BST-RULE-0053",
            "Compile-time evaluation error",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidCallShape => DiagnosticDescriptor::new(
            "BST-RULE-0054",
            "Invalid call shape",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidReturnShape => DiagnosticDescriptor::new(
            "BST-RULE-0055",
            "Invalid return shape",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidFunctionSignature => DiagnosticDescriptor::new(
            "BST-RULE-0062",
            "Invalid function signature",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidGenericInstantiation => DiagnosticDescriptor::new(
            "BST-RULE-0057",
            "Invalid generic instantiation",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::UnsupportedExternalFunction => DiagnosticDescriptor::new(
            "BST-RULE-0058",
            "Unsupported external function",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidRangeOperand => DiagnosticDescriptor::new(
            "BST-RULE-0059",
            "Invalid range operand",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::UnsupportedBuilderPackage => DiagnosticDescriptor::new(
            "BST-RULE-0060",
            "Unsupported builder package",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::UnsupportedBackendFeature => DiagnosticDescriptor::new(
            "BST-RULE-0064",
            "Unsupported backend feature",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidPageMetadata => DiagnosticDescriptor::new(
            "BST-RULE-0061",
            "Invalid page metadata",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidCompileTimePath => DiagnosticDescriptor::new(
            "BST-RULE-0063",
            "Invalid compile-time path",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::ImportRecordUsedAsValue => DiagnosticDescriptor::new(
            "BST-RULE-0065",
            "Import record used as value",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::ConstRecordUsedAsValue => DiagnosticDescriptor::new(
            "BST-RULE-0068",
            "Const record used as value",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::NestedTraversal => DiagnosticDescriptor::new(
            "BST-RULE-0066",
            "Nested import-record traversal",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::NamespaceTypeValueMisuse => DiagnosticDescriptor::new(
            "BST-RULE-0067",
            "Namespace type/value misuse",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::UnknownTrait => {
            DiagnosticDescriptor::new("BST-RULE-0069", "Unknown trait", DiagnosticSeverity::Error)
        }
        RuleDiagnosticKind::DuplicateTraitRequirement => DiagnosticDescriptor::new(
            "BST-RULE-0070",
            "Duplicate trait requirement",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::TraitPrivateSurfaceLeak => DiagnosticDescriptor::new(
            "BST-RULE-0071",
            "Private type exposed by trait",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::UnsupportedTraitFeature => DiagnosticDescriptor::new(
            "BST-RULE-0072",
            "Unsupported trait feature",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidTraitConformance => DiagnosticDescriptor::new(
            "BST-RULE-0073",
            "Invalid trait conformance",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::GenericBoundPrivateSurfaceLeak => DiagnosticDescriptor::new(
            "BST-RULE-0074",
            "Private trait exposed by generic bound",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidDynamicTraitType => DiagnosticDescriptor::new(
            "BST-RULE-0075",
            "Invalid dynamic trait type",
            DiagnosticSeverity::Error,
        ),
        RuleDiagnosticKind::InvalidTraitKeywordUsage => DiagnosticDescriptor::new(
            "BST-RULE-0076",
            "Invalid trait keyword usage",
            DiagnosticSeverity::Error,
        ),
    }
}

fn import_descriptor(kind: ImportDiagnosticKind) -> DiagnosticDescriptor {
    match kind {
        ImportDiagnosticKind::UnusedImport => DiagnosticDescriptor::new(
            "BST-IMPORT-0002",
            "Unused import",
            DiagnosticSeverity::Warning,
        ),
        ImportDiagnosticKind::ImportAliasCaseMismatch => DiagnosticDescriptor::new(
            "BST-IMPORT-0003",
            "Import alias case mismatch",
            DiagnosticSeverity::Warning,
        ),

        ImportDiagnosticKind::MissingImportTarget => DiagnosticDescriptor::new(
            "BST-IMPORT-0005",
            "Missing import target",
            DiagnosticSeverity::Error,
        ),
        ImportDiagnosticKind::AmbiguousImportTarget => DiagnosticDescriptor::new(
            "BST-IMPORT-0006",
            "Ambiguous import target",
            DiagnosticSeverity::Error,
        ),
        ImportDiagnosticKind::BareFileImport => DiagnosticDescriptor::new(
            "BST-IMPORT-0007",
            "Bare file import",
            DiagnosticSeverity::Error,
        ),
        ImportDiagnosticKind::DirectSpecialFileImport => DiagnosticDescriptor::new(
            "BST-IMPORT-0008",
            "Direct special file import",
            DiagnosticSeverity::Error,
        ),
        ImportDiagnosticKind::ImportNameCollision => DiagnosticDescriptor::new(
            "BST-IMPORT-0009",
            "Import name collision",
            DiagnosticSeverity::Error,
        ),
        ImportDiagnosticKind::NotExportedBySourceFile => DiagnosticDescriptor::new(
            "BST-IMPORT-0010",
            "Not exported by source file",
            DiagnosticSeverity::Error,
        ),
        ImportDiagnosticKind::NotExportedByFacade => DiagnosticDescriptor::new(
            "BST-IMPORT-0011",
            "Not exported by facade",
            DiagnosticSeverity::Error,
        ),
        ImportDiagnosticKind::MissingModuleFacade => DiagnosticDescriptor::new(
            "BST-IMPORT-0012",
            "Missing module facade",
            DiagnosticSeverity::Error,
        ),
        ImportDiagnosticKind::MissingPackageSymbol => DiagnosticDescriptor::new(
            "BST-IMPORT-0013",
            "Missing package symbol",
            DiagnosticSeverity::Error,
        ),
        ImportDiagnosticKind::CrossModuleImportNotExported => DiagnosticDescriptor::new(
            "BST-IMPORT-0015",
            "Cross-module import not exported",
            DiagnosticSeverity::Error,
        ),
        ImportDiagnosticKind::InvalidImportPath => DiagnosticDescriptor::new(
            "BST-IMPORT-0016",
            "Invalid import path",
            DiagnosticSeverity::Error,
        ),
        ImportDiagnosticKind::DirectSymbolPathImport => DiagnosticDescriptor::new(
            "BST-IMPORT-0017",
            "Direct symbol-path import",
            DiagnosticSeverity::Error,
        ),
        ImportDiagnosticKind::InvalidNamespaceDefaultName => DiagnosticDescriptor::new(
            "BST-IMPORT-0018",
            "Invalid namespace default name",
            DiagnosticSeverity::Error,
        ),
        ImportDiagnosticKind::DuplicateImportSurfaceMember => DiagnosticDescriptor::new(
            "BST-IMPORT-0019",
            "Duplicate import surface member",
            DiagnosticSeverity::Error,
        ),
        ImportDiagnosticKind::ExplicitBstExtension => DiagnosticDescriptor::new(
            "BST-IMPORT-0020",
            "Explicit .bst extension in import",
            DiagnosticSeverity::Error,
        ),
        ImportDiagnosticKind::UnsupportedExternalExtension => DiagnosticDescriptor::new(
            "BST-IMPORT-0021",
            "Unsupported external file extension in import",
            DiagnosticSeverity::Error,
        ),
        ImportDiagnosticKind::InvalidExternalLibrary => DiagnosticDescriptor::new(
            "BST-IMPORT-0022",
            "Invalid external library",
            DiagnosticSeverity::Error,
        ),
        ImportDiagnosticKind::ReceiverMethodImportRequiresVisibleReceiverType => {
            DiagnosticDescriptor::new(
                "BST-IMPORT-0023",
                "Receiver method import requires visible receiver type",
                DiagnosticSeverity::Error,
            )
        }
    }
}

fn borrow_descriptor(kind: BorrowDiagnosticKind) -> DiagnosticDescriptor {
    match kind {
        BorrowDiagnosticKind::BorrowConflict => DiagnosticDescriptor::new(
            "BST-BORROW-0001",
            "Borrow conflict",
            DiagnosticSeverity::Error,
        ),
        BorrowDiagnosticKind::MultipleMutableBorrows => DiagnosticDescriptor::new(
            "BST-BORROW-0002",
            "Multiple mutable borrows",
            DiagnosticSeverity::Error,
        ),
        BorrowDiagnosticKind::SharedMutableConflict => DiagnosticDescriptor::new(
            "BST-BORROW-0003",
            "Shared and mutable access conflict",
            DiagnosticSeverity::Error,
        ),
        BorrowDiagnosticKind::UseAfterPossibleMove => DiagnosticDescriptor::new(
            "BST-BORROW-0004",
            "Use after possible move",
            DiagnosticSeverity::Error,
        ),
        BorrowDiagnosticKind::MoveWhileBorrowed => DiagnosticDescriptor::new(
            "BST-BORROW-0005",
            "Move while borrowed",
            DiagnosticSeverity::Error,
        ),
        BorrowDiagnosticKind::WholeObjectBorrowConflict => DiagnosticDescriptor::new(
            "BST-BORROW-0006",
            "Whole-object borrow conflict",
            DiagnosticSeverity::Error,
        ),
        BorrowDiagnosticKind::InvalidMutableAccess => DiagnosticDescriptor::new(
            "BST-BORROW-0007",
            "Invalid mutable access",
            DiagnosticSeverity::Error,
        ),
        BorrowDiagnosticKind::InvalidAccessAfterPossibleOwnershipTransfer => {
            DiagnosticDescriptor::new(
                "BST-BORROW-0008",
                "Invalid access after possible ownership transfer",
                DiagnosticSeverity::Error,
            )
        }
        BorrowDiagnosticKind::UseOfUninitializedLocal => DiagnosticDescriptor::new(
            "BST-BORROW-0009",
            "Use of uninitialized local",
            DiagnosticSeverity::Error,
        ),
    }
}

fn config_descriptor(kind: ConfigDiagnosticKind) -> DiagnosticDescriptor {
    match kind {
        ConfigDiagnosticKind::InvalidConfig => DiagnosticDescriptor::new(
            "BST-CONFIG-0001",
            "Invalid config",
            DiagnosticSeverity::Error,
        ),
    }
}

fn infrastructure_descriptor(kind: InfrastructureDiagnosticKind) -> DiagnosticDescriptor {
    match kind {
        InfrastructureDiagnosticKind::InfrastructureFailure => DiagnosticDescriptor::new(
            "BST-INFRA-0001",
            "Infrastructure failure",
            DiagnosticSeverity::Error,
        ),
    }
}

fn deferred_feature_descriptor(kind: DeferredFeatureDiagnosticKind) -> DiagnosticDescriptor {
    match kind {
        DeferredFeatureDiagnosticKind::DeferredFeature => DiagnosticDescriptor::new(
            "BST-DEFERRED-0001",
            "Deferred feature",
            DiagnosticSeverity::Error,
        ),
    }
}
