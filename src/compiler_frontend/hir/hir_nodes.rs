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

use super::hir_display::HirSideTable;
use crate::backends::function_registry::CallTarget;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::hir::hir_datatypes::{TypeContext, TypeId};
use crate::compiler_frontend::tokenizer::tokens::TextLocation;

// ============================================================
// Stable IDs
// ============================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HirNodeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HirValueId(pub u32);

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
    pub type_context: TypeContext,
    pub side_table: HirSideTable,

    /// Entry point for execution.
    pub start_function: FunctionId,

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
    kind: RegionKind,
}

#[derive(Debug, Clone)]
enum RegionKind {
    Lexical,   // compiler-generated
    UserArena, // explicit syntax (POSSIBLE FUTURE EXTENSION TO THE LANGUAGE)
}

impl HirRegion {
    pub(crate) fn lexical(id: RegionId, parent: Option<RegionId>) -> Self {
        Self {
            id,
            parent,
            kind: RegionKind::Lexical,
        }
    }

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

    pub(crate) fn is_lexical(&self) -> bool {
        matches!(self.kind, RegionKind::Lexical)
    }

    pub(crate) fn is_user_arena(&self) -> bool {
        matches!(self.kind, RegionKind::UserArena)
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
    pub location: TextLocation,
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
