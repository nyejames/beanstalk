//! ============================================================
//!                         HIR Nodes
//! ============================================================
//! This module defines the frontend HIR data model.
//!
//! HIR is Beanstalk's typed semantic IR between AST and backend lowering:
//! - control flow is explicit via blocks/statements/terminators
//! - locals, regions, and symbols are keyed by stable IDs
//! - source/name/type metadata is carried through side tables
//! - expression trees are still allowed for normal operators/value construction
//!
//! HIR strips AST parsing-only machinery from normal lowering paths.
//! Template parsing, folding, and runtime render-plan construction belong to AST.
//! HIR only consumes the finalized semantic template data AST hands it.
//!
//! ============================================================
//!                     Memory Semantics
//! ============================================================
//!
//! All heap values are GC references by default.
//! Ownership is a runtime optimisation, not a type distinction.
//! HIR provides:
//!   - RegionId for lifetime analysis
//!   - Mutability flags for exclusivity checking
//!
//! Ownership/borrow analysis runs as a separate pass keyed by HIR IDs.
//! See: docs/memory-management-design.md
//! The analysis phases AFTER the HIR creation are responsible for giving the project builder
//! info about where it could insert possible_drops, drops, or other optimisations.
//!
//! HIR is designed to support both models:
//! - Ownership annotations are **advisory hints** for optimisation, not semantic requirements
//! - All programs are correct under pure GC interpretation
//! - Static analysis strengthens guarantees incrementally without changing HIR structure
//!
//! ============================================================
//!                     Multiple Returns
//! ============================================================
//!
//! Beanstalk supports multiple return values (Go-style).
//! Functions can return multiple unwrapped values.
//!
//! These can be wrapped in Option or Result at the signature level:
//! - `fn || -> Int, String` → returns two values
//! - `fn || -> Int, String?` → returns Option<(Int, String)>
//! - `fn || -> Int, String!` → returns Result<(Int, String), Error>

use super::hir_side_table::HirSideTable;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::hir::hir_datatypes::{TypeContext, TypeId};
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

// ============================================================
// Stable IDs
// ============================================================
macro_rules! define_hir_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(pub u32);
    };
}

define_hir_id!(HirNodeId);
define_hir_id!(HirValueId);
define_hir_id!(BlockId);
define_hir_id!(LocalId);
define_hir_id!(StructId);
define_hir_id!(FieldId);
define_hir_id!(FunctionId);
define_hir_id!(RegionId);
define_hir_id!(HirConstId);
define_hir_id!(ChoiceId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HirFunctionOrigin {
    /// Regular user-declared function.
    Normal,
    /// Implicit start function for the module entry file.
    EntryStart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HirDocFragmentKind {
    Doc,
}

#[derive(Debug, Clone)]
pub struct HirDocFragment {
    pub kind: HirDocFragmentKind,
    #[allow(dead_code)] // Used only in tests
    pub rendered_text: String,
    pub location: SourceLocation,
}

#[derive(Debug, Clone)]
pub struct HirConstField {
    pub name: String,
    pub value: HirConstValue,
}

#[derive(Debug, Clone)]
pub enum HirConstValue {
    #[allow(dead_code)]
    // Stored during lowering; scalar payloads are not inspected in Alpha validation.
    Int(i64),
    #[allow(dead_code)]
    // Stored during lowering; scalar payloads are not inspected in Alpha validation.
    Float(f64),
    #[allow(dead_code)]
    // Stored during lowering; scalar payloads are not inspected in Alpha validation.
    Bool(bool),
    #[allow(dead_code)]
    // Stored during lowering; scalar payloads are not inspected in Alpha validation.
    Char(char),
    String(String),
    Collection(Vec<HirConstValue>),
    Record(Vec<HirConstField>),
    Range(Box<HirConstValue>, Box<HirConstValue>),
    Result {
        #[allow(dead_code)]
        // Variant is stored during lowering; Alpha validation only checks the value.
        variant: ResultVariant,
        value: Box<HirConstValue>,
    },
}

#[derive(Debug, Clone)]
pub struct HirModuleConst {
    pub id: HirConstId,
    pub name: String,
    pub ty: TypeId,
    pub value: HirConstValue,
}

// ============================================================
// Module
// ============================================================
/// Registry entry for a nominal choice type.
///
/// WHY: the `choices` vec provides a dense `ChoiceId` namespace.
/// Alpha scope supports unit variants only; payload fields are intentionally omitted.
#[derive(Debug, Clone)]
pub struct HirChoice {
    #[allow(dead_code)]
    // Stored during lowering; existence checked by ChoiceId index in validation.
    pub id: ChoiceId,
    #[allow(dead_code)] // Stored during lowering; not walked in Alpha validation.
    pub variants: Vec<HirChoiceVariant>,
}

#[derive(Debug, Clone)]
pub struct HirChoiceVariant {
    #[allow(dead_code)] // Stored during lowering; not read back in Alpha paths.
    pub name: StringId,
}

#[derive(Debug, Clone)]
pub struct HirModule {
    pub blocks: Vec<HirBlock>,
    pub functions: Vec<HirFunction>,
    pub structs: Vec<HirStruct>,
    pub choices: Vec<HirChoice>,
    pub type_context: TypeContext,
    pub side_table: HirSideTable,

    /// Entry point for execution.
    pub start_function: FunctionId,
    /// Classification for every function in the module.
    ///
    /// WHY: backends/builders need explicit semantic role tagging to keep
    /// entry/runtime-template behavior stable across lowering passes.
    pub function_origins: rustc_hash::FxHashMap<FunctionId, HirFunctionOrigin>,

    pub doc_fragments: Vec<HirDocFragment>,
    pub module_constants: Vec<HirModuleConst>,
    pub rendered_path_usages: Vec<RenderedPathUsage>,

    /// Region tree
    pub regions: Vec<HirRegion>,

    /// Warnings Collected along the way
    pub warnings: Vec<CompilerWarning>,
}

impl HirModule {
    pub fn new() -> Self {
        Self {
            blocks: vec![],
            functions: vec![],
            structs: vec![],
            choices: vec![],
            type_context: TypeContext::default(),
            side_table: HirSideTable::default(),
            start_function: FunctionId(0),
            function_origins: rustc_hash::FxHashMap::default(),
            doc_fragments: vec![],
            module_constants: vec![],
            rendered_path_usages: vec![],
            regions: vec![],
            warnings: vec![],
        }
    }

    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.side_table.remap_string_ids(remap);

        for fragment in &mut self.doc_fragments {
            fragment.location.remap_string_ids(remap);
        }

        for usage in &mut self.rendered_path_usages {
            usage.source_path.remap_string_ids(remap);
            usage.public_path.remap_string_ids(remap);
            usage.source_file_scope.remap_string_ids(remap);
            usage.render_location.remap_string_ids(remap);
        }

        for warning in &mut self.warnings {
            warning.remap_string_ids(remap);
        }
    }
}

// ============================================================
// Regions
// ============================================================
#[derive(Debug, Clone)]
pub struct HirRegion {
    id: RegionId,
    parent: Option<RegionId>,
}

impl HirRegion {
    pub(crate) fn lexical(id: RegionId, parent: Option<RegionId>) -> Self {
        Self { id, parent }
    }

    pub fn id(&self) -> RegionId {
        self.id
    }

    pub fn parent(&self) -> Option<RegionId> {
        self.parent
    }
}

// ============================================================
// Structs
// ============================================================
#[derive(Debug, Clone)]
pub struct HirStruct {
    pub id: StructId,
    pub fields: Vec<HirField>,
}

#[derive(Debug, Clone)]
pub struct HirField {
    pub id: FieldId,
    pub ty: TypeId,
}

// ============================================================
// Functions
// ============================================================
#[derive(Debug, Clone)]
pub struct HirFunction {
    pub id: FunctionId,
    pub entry: BlockId,
    pub params: Vec<LocalId>,
    pub return_type: TypeId,
    pub return_aliases: Vec<Option<Vec<usize>>>,
}

// ============================================================
// Blocks
// ============================================================
#[derive(Debug, Clone)]
pub struct HirBlock {
    pub id: BlockId,
    pub region: RegionId,

    /// All locals declared within this block.
    pub locals: Vec<HirLocal>,

    pub statements: Vec<HirStatement>,
    pub terminator: HirTerminator,
}

#[derive(Debug, Clone)]
pub struct HirLocal {
    pub id: LocalId,
    pub ty: TypeId,
    pub mutable: bool,
    pub region: RegionId,
    pub source_info: Option<SourceLocation>,
}

// ============================================================
// Places (Canonical Memory Projection)
// ============================================================
#[derive(Debug, Clone)]
pub enum HirPlace {
    Local(LocalId),

    Field {
        base: Box<HirPlace>,
        field: FieldId,
    },

    Index {
        base: Box<HirPlace>,
        index: Box<HirExpression>,
    },
}

// ============================================================
// Statements
// ============================================================
#[derive(Debug, Clone)]
pub struct HirStatement {
    pub id: HirNodeId,
    pub kind: HirStatementKind,
    pub location: SourceLocation,
}

#[derive(Debug, Clone)]
pub enum HirStatementKind {
    Assign {
        target: HirPlace,
        value: HirExpression,
    },

    // HIR construction flattens nested calls.
    // Single-call expressions don't need explicit assignment in the source
    Call {
        target: CallTarget,
        args: Vec<HirExpression>,
        result: Option<LocalId>,
    },

    /// Expression evaluated only for side effects.
    Expr(HirExpression),

    /// Accumulate one runtime string value into the entry start() fragment vec.
    ///
    /// WHAT: explicit HIR primitive that lowers from `NodeKind::PushStartRuntimeFragment`.
    /// WHY: backends handle fragment accumulation without needing to inspect the entry start
    /// function body for heuristic push patterns.
    PushRuntimeFragment {
        /// The local holding the Vec<String> accumulator inside entry start().
        vec_local: LocalId,
        /// Expression that produces the string value to push.
        value: HirExpression,
    },

    /// Explicit deterministic drop.
    #[allow(dead_code)] // Planned: explicit drop statements after ownership lowering matures.
    Drop(LocalId),
}

// ============================================================
// Terminators (Explicit Control Flow)
// ============================================================
#[derive(Debug, Clone)]
pub enum HirTerminator {
    Jump {
        target: BlockId,
        args: Vec<LocalId>, // Not SSA - just passing current local values
    },

    If {
        condition: HirExpression,
        then_block: BlockId,
        else_block: BlockId, // Required, must jump or return somewhere (Could just be continuation)
    },

    Match {
        scrutinee: HirExpression,
        arms: Vec<HirMatchArm>, // Each arm's body block must end with Jump or Return
    },

    Break {
        target: BlockId,
    },

    Continue {
        target: BlockId,
    },

    Return(HirExpression),

    Panic {
        message: Option<HirExpression>,
    },
}

// ============================================================
// Expressions
// ============================================================
#[derive(Debug, Clone)]
pub struct HirExpression {
    pub id: HirValueId,
    pub kind: HirExpressionKind,
    pub ty: TypeId,
    pub value_kind: ValueKind,
    pub region: RegionId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    /// Refers to a memory location.
    Place,

    /// Produces a value.
    RValue,

    /// Compile-time constant.
    Const,
}

#[derive(Debug, Clone)]
pub enum HirExpressionKind {
    // --------------------------------------------------------
    // Literals
    // --------------------------------------------------------
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    StringLiteral(String),

    // --------------------------------------------------------
    // Memory
    // --------------------------------------------------------
    Load(HirPlace),
    Copy(HirPlace),

    // --------------------------------------------------------
    // Operations
    // --------------------------------------------------------
    BinOp {
        left: Box<HirExpression>,
        op: HirBinOp,
        right: Box<HirExpression>,
    },

    UnaryOp {
        op: HirUnaryOp,
        operand: Box<HirExpression>,
    },

    // --------------------------------------------------------
    // Construction
    // --------------------------------------------------------
    StructConstruct {
        struct_id: StructId,
        fields: Vec<(FieldId, HirExpression)>,
    },

    Collection(Vec<HirExpression>),

    Range {
        start: Box<HirExpression>,
        end: Box<HirExpression>,
    },

    /// Construct a tuple value (for multi-return)
    /// Example: return (42, "hello")
    /// EMPTY TUPLE IS THE UNIT TYPE ()
    /// EMPTY TUPLE == DataType::None
    TupleConstruct {
        elements: Vec<HirExpression>,
    },

    /// Project a tuple slot by flat index.
    TupleGet {
        tuple: Box<HirExpression>,
        index: usize,
    },

    ///Construct an Option value
    /// - Some variant: value must be Some(expr)
    /// - None variant: value must be None
    OptionConstruct {
        variant: OptionVariant,
        value: Option<Box<HirExpression>>, // None for None variant, Some for Some variant
    },

    /// Construct a Result value
    /// Example: Ok(42) or Err("error")
    ResultConstruct {
        variant: ResultVariant,
        value: Box<HirExpression>, // The wrapped value
    },

    /// Unwraps an internal Result value for `call(...)!` propagation:
    /// - Ok(v)  => evaluates to v
    /// - Err(e) => propagates through the current function's error channel
    ResultPropagate {
        result: Box<HirExpression>,
    },

    /// Checks whether an internal Result carrier currently holds an Ok value.
    ResultIsOk {
        result: Box<HirExpression>,
    },

    /// Extracts the Ok payload from an internal Result carrier.
    ResultUnwrapOk {
        result: Box<HirExpression>,
    },

    /// Extracts the Err payload from an internal Result carrier.
    ResultUnwrapErr {
        result: Box<HirExpression>,
    },

    BuiltinCast {
        kind: HirBuiltinCastKind,
        value: Box<HirExpression>,
    },

    /// Explicit choice variant value.
    ///
    /// WHY: choice tags are nominal, not raw integers. A dedicated HIR node
    /// preserves choice identity for backend lowering and future payload support.
    ChoiceVariant {
        choice_id: ChoiceId,
        variant_index: usize,
    },
}

// ============================================================
// Pattern Matching
// ============================================================
#[derive(Debug, Clone)]
pub struct HirMatchArm {
    pub pattern: HirPattern,
    pub guard: Option<HirExpression>,
    pub body: BlockId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirRelationalPatternOp {
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

#[derive(Debug, Clone)]
pub enum HirPattern {
    Literal(HirExpression),
    Wildcard,
    Relational {
        op: HirRelationalPatternOp,
        value: HirExpression,
    },
    ChoiceVariant {
        choice_id: ChoiceId,
        variant_index: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptionVariant {
    #[allow(dead_code)]
    // Kept until alpha Option<T> lowering emits explicit Some carriers.
    Some,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HirBuiltinCastKind {
    Int,
    Float,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResultVariant {
    Ok,
    Err,
}

// ============================================================
// Operators
// ============================================================
#[derive(Debug, Clone, Copy)]
pub enum HirBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    IntDiv,
    Exponent,
}

#[derive(Debug, Clone, Copy)]
pub enum HirUnaryOp {
    Neg,
    Not,
}
