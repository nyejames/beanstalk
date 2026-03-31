//! ============================================================
//!                         HIR Nodes
//! ============================================================
//! A Fully resolved, canonical semantic representation of Beanstalk programs.
//!  - All symbols are resolved to stable IDs
//!  - All expressions fully typed
//!  - Explicit locals and regions
//!  - No AST artefacts
//!  - No inference remnants
//!
//! This module defines the High-Level Intermediate Representation (HIR) for Beanstalk.
//! HIR is a structured, semantically rich IR designed for borrow checking, move analysis,
//! and preparing code for reliable lowering to multiple backends.
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
//! Ownership analysis runs as a separate pass keyed by HIR IDs.
//! See: docs/Beanstalk Memory Management.md
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
define_hir_id!(ConstStringId);
define_hir_id!(HirConstId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StartFragment {
    ConstString(ConstStringId),
    RuntimeStringFn(FunctionId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HirFunctionOrigin {
    /// Regular user-declared function.
    Normal,
    /// Implicit start function for the module entry file.
    EntryStart,
    /// Implicit start function for non-entry imported file.
    FileStart,
    /// Runtime template fragment function synthesized from top-level templates.
    RuntimeTemplate,
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
    #[allow(dead_code)] // Planned: integer constant payloads during extended const lowering.
    Int(i64),
    #[allow(dead_code)] // Planned: float constant payloads during extended const lowering.
    Float(f64),
    #[allow(dead_code)] // Planned: boolean constant payloads during extended const lowering.
    Bool(bool),
    #[allow(dead_code)] // Planned: char constant payloads during extended const lowering.
    Char(char),
    #[allow(dead_code)] // Planned: string constant payloads during extended const lowering.
    String(String),
    Collection(Vec<HirConstValue>),
    Record(Vec<HirConstField>),
    Range(Box<HirConstValue>, Box<HirConstValue>),
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
#[derive(Debug, Clone)]
pub struct HirModule {
    pub blocks: Vec<HirBlock>,
    pub functions: Vec<HirFunction>,
    pub structs: Vec<HirStruct>,
    pub type_context: TypeContext,
    pub side_table: HirSideTable,

    /// Entry point for execution.
    pub start_function: FunctionId,
    /// Classification for every function in the module.
    ///
    /// WHY: backends/builders need explicit semantic role tagging to keep
    /// entry/runtime-template behavior stable across lowering passes.
    pub function_origins: rustc_hash::FxHashMap<FunctionId, HirFunctionOrigin>,

    /// Ordered start-fragment stream consumed by project builders.
    pub start_fragments: Vec<StartFragment>,
    pub const_string_pool: Vec<String>,
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
            type_context: TypeContext::default(),
            side_table: HirSideTable::default(),
            start_function: FunctionId(0),
            function_origins: rustc_hash::FxHashMap::default(),
            start_fragments: vec![],
            const_string_pool: vec![],
            doc_fragments: vec![],
            module_constants: vec![],
            rendered_path_usages: vec![],
            regions: vec![],
            warnings: vec![],
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
    #[allow(dead_code)] // Planned: user-defined region arenas in future memory model phases.
    kind: RegionKind,
}

#[derive(Debug, Clone)]
enum RegionKind {
    Lexical, // compiler-generated
    #[allow(dead_code)] // Planned: explicit user arena regions.
    UserArena,
}

impl HirRegion {
    pub(crate) fn lexical(id: RegionId, parent: Option<RegionId>) -> Self {
        Self {
            id,
            parent,
            kind: RegionKind::Lexical,
        }
    }

    #[allow(dead_code)] // Planned: user arena region construction.
    pub(crate) fn user_arena(id: RegionId, parent: Option<RegionId>) -> Self {
        Self {
            id,
            parent,
            kind: RegionKind::UserArena,
        }
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

    #[allow(dead_code)] // Planned: indexed place projections for collection/tuple accesses.
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

    #[allow(dead_code)] // Planned: canonical loop terminator for structured loop lowering.
    Loop {
        body: BlockId,
        break_target: BlockId, // Explicit break destination
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

    ///Construct an Option value
    /// - Some variant: value must be Some(expr)
    /// - None variant: value must be None
    #[allow(dead_code)] // Planned: Option value construction in HIR.
    OptionConstruct {
        variant: OptionVariant,
        value: Option<Box<HirExpression>>, // None for None variant, Some for Some variant
    },

    /// Construct a Result value
    /// Example: Ok(42) or Err("error")
    #[allow(dead_code)] // Planned: Result value construction in HIR.
    ResultConstruct {
        variant: ResultVariant,
        value: Box<HirExpression>, // The wrapped value
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

#[derive(Debug, Clone)]
pub enum HirPattern {
    Literal(HirExpression),
    Wildcard,

    #[allow(dead_code)] // Planned: binding patterns for match/destructuring support.
    Binding {
        local: LocalId,
        subpattern: Option<Box<HirPattern>>,
    },

    #[allow(dead_code)] // Planned: struct destructuring patterns.
    Struct {
        struct_id: StructId,
        fields: Vec<(FieldId, HirPattern)>,
    },

    /// Match tuples/multiple returns
    /// Essential for destructuring multi-return in Option/Result
    #[allow(dead_code)] // Planned: tuple destructuring patterns.
    Tuple {
        elements: Vec<HirPattern>,
    },

    /// Match Option<T>
    #[allow(dead_code)] // Planned: Option pattern matching.
    Option {
        variant: OptionVariant,
        inner_pattern: Option<Box<HirPattern>>, // Pattern for the Some value
    },

    /// Match Result<T, E>
    #[allow(dead_code)] // Planned: Result pattern matching.
    Result {
        variant: ResultVariant,
        inner_pattern: Option<Box<HirPattern>>, // Pattern for Ok/Err value
    },

    /// Match collections
    #[allow(dead_code)] // Planned: collection destructuring patterns with rest capture.
    Collection {
        elements: Vec<HirPattern>,
        rest: Option<LocalId>, // For [x, y, ..rest] patterns
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptionVariant {
    #[allow(dead_code)] // Planned: Option::Some variant handling.
    Some,
    #[allow(dead_code)] // Planned: Option::None variant handling.
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResultVariant {
    #[allow(dead_code)] // Planned: Result::Ok variant handling.
    Ok,
    #[allow(dead_code)] // Planned: Result::Err variant handling.
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
    Root,
    Exponent,
}

#[derive(Debug, Clone, Copy)]
pub enum HirUnaryOp {
    Neg,
    Not,
}
