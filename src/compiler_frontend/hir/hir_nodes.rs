//! ============================================================
//!                         HIR Nodes
//! ============================================================
//! A Fully resolved, canonical semantic representation of Beanstalk programs.
//!  - All symbols are resolved to stable IDs
//!  - All expressions fully typed
//!  - Explicit locals and regions
//!  - No AST artifacts
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
//! Ownership is a runtime optimization, not a type distinction.
//! HIR provides:
//!   - RegionId for lifetime analysis
//!   - Mutability flags for exclusivity checking
//!   - Drop statements for possible_drop insertion
//!
//! Ownership analysis runs as a separate pass keyed by HIR IDs.
//! See: docs/Beanstalk Memory Management.md
//!
//! HIR is designed to support both models:
//! - Ownership annotations are **advisory hints** for optimization, not semantic requirements
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

use crate::backends::function_registry::CallTarget;
use crate::compiler_frontend::hir::hir_datatypes::TypeId;
use crate::compiler_frontend::tokenizer::tokens::TextLocation;

// ============================================================
// Stable IDs
// ============================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HirNodeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegionId(pub u32);

// ============================================================
// Module
// ============================================================
#[derive(Debug, Clone)]
pub struct HirModule {
    pub blocks: Vec<HirBlock>,
    pub functions: Vec<HirFunction>,
    pub structs: Vec<HirStruct>,

    /// Entry point for execution.
    pub start_function: FunctionId,

    /// Region tree
    pub regions: Vec<HirRegion>,
}

// ============================================================
// Regions
// ============================================================
#[derive(Debug, Clone)]
pub struct HirRegion {
    id: RegionId,
    parent: Option<RegionId>,
    kind: RegionKind,
}

#[derive(Debug, Clone)]
enum RegionKind {
    Lexical,   // compiler-generated
    UserArena, // explicit syntax (POSSIBLE FUTURE EXTENSION TO THE LANGUAGE)
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

    pub statements: Vec<HirStmt>,
    pub terminator: HirTerminator,
}

#[derive(Debug, Clone)]
pub struct HirLocal {
    pub id: LocalId,
    pub ty: TypeId,
    pub mutable: bool,
    pub region: RegionId,
    pub source_info: Option<TextLocation>,
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
        index: Box<HirExpr>,
    },
}

// ============================================================
// Statements
// ============================================================
#[derive(Debug, Clone)]
pub struct HirStmt {
    pub id: HirNodeId,
    pub kind: HirStmtKind,
    pub location: TextLocation,
}

#[derive(Debug, Clone)]
pub enum HirStmtKind {
    Assign {
        target: HirPlace,
        value: HirExpr,
    },

    // HIR construction flattens nested calls.
    // Single-call expressions don't need explicit assignment in the source
    Call {
        target: CallTarget,
        args: Vec<HirExpr>,
        result: Option<LocalId>,
    },

    /// Expression evaluated only for side effects.
    Expr(HirExpr),

    /// Explicit deterministic drop.
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
        condition: HirExpr,
        then_block: BlockId,
        else_block: BlockId, // Required, must jump or return somewhere (Could just be continuation)
    },

    Match {
        scrutinee: HirExpr,
        arms: Vec<HirMatchArm>, // Each arm's body block must end with Jump or Return
    },

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

    Return(HirExpr),

    Panic {
        message: Option<HirExpr>,
    },
}

// ============================================================
// Expressions
// ============================================================
#[derive(Debug, Clone)]
pub struct HirExpr {
    pub kind: HirExprKind,
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
pub enum HirExprKind {
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

    // --------------------------------------------------------
    // Operations
    // --------------------------------------------------------
    BinOp {
        left: Box<HirExpr>,
        op: BinOp,
        right: Box<HirExpr>,
    },

    UnaryOp {
        op: UnaryOp,
        operand: Box<HirExpr>,
    },

    // --------------------------------------------------------
    // Construction
    // --------------------------------------------------------
    StructConstruct {
        struct_id: StructId,
        fields: Vec<(FieldId, HirExpr)>,
    },

    Collection(Vec<HirExpr>),

    Range {
        start: Box<HirExpr>,
        end: Box<HirExpr>,
    },

    /// Construct a tuple value (for multi-return)
    /// Example: return (42, "hello")
    /// EMPTY TUPLE IS THE UNIT TYPE ()
    /// EMPTY TUPLE == DataType::None
    TupleConstruct {
        elements: Vec<HirExpr>,
    },

    ///Construct an Option value
    /// - Some variant: value must be Some(expr)
    /// - None variant: value must be None
    OptionConstruct {
        variant: OptionVariant,
        value: Option<Box<HirExpr>>, // None for None variant, Some for Some variant
    },

    /// Construct a Result value
    /// Example: Ok(42) or Err("error")
    ResultConstruct {
        variant: ResultVariant,
        value: Box<HirExpr>, // The wrapped value
    },
}

// ============================================================
// Pattern Matching
// ============================================================
#[derive(Debug, Clone)]
pub struct HirMatchArm {
    pub pattern: HirPattern,
    pub guard: Option<HirExpr>,
    pub body: BlockId,
}

#[derive(Debug, Clone)]
pub enum HirPattern {
    Literal(HirExpr),
    Wildcard,

    Binding {
        local: LocalId,
        subpattern: Option<Box<HirPattern>>,
    },

    Struct {
        struct_id: StructId,
        fields: Vec<(FieldId, HirPattern)>,
    },

    /// Match tuples/multiple returns
    /// Essential for destructuring multi-return in Option/Result
    Tuple {
        elements: Vec<HirPattern>,
    },

    /// Match Option<T>
    Option {
        variant: OptionVariant,
        inner_pattern: Option<Box<HirPattern>>, // Pattern for the Some value
    },

    /// Match Result<T, E>
    Result {
        variant: ResultVariant,
        inner_pattern: Option<Box<HirPattern>>, // Pattern for Ok/Err value
    },

    /// Match collections
    Collection {
        elements: Vec<HirPattern>,
        rest: Option<LocalId>, // For [x, y, ..rest] patterns
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptionVariant {
    Some,
    None,
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
pub enum BinOp {
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
pub enum UnaryOp {
    Neg,
    Not,
}

// ============================================================
// Debug Overlay
// ============================================================
//
// Semantic identity is ID-based.
// Names are stored separately for diagnostics.
pub struct HirDebug {
    pub locals: Vec<String>,    // indexed by LocalId
    pub functions: Vec<String>, // indexed by FunctionId
    pub structs: Vec<String>,   // indexed by StructId
}
