//! HIR core node definitions
//!
//! This module defines the High-Level Intermediate Representation (HIR) for Beanstalk.
//! HIR is a structured, semantically rich IR designed for borrow checking, move analysis,
//! and preparing code for reliable lowering to WebAssembly.
//!
//! Key design principles:
//! - Structured control flow for CFG-based analysis
//! - Place-based memory model for precise borrow tracking
//! - No nested expressions - all computation linearized into statements
//! - Borrow intent, not ownership outcome (determined by the borrow checker)
//! - Language-shaped, not Wasm-shaped (deferred to LIR)

use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::InternedString;

#[derive(Debug, Clone)]
pub struct HirNode {
    pub kind: HirKind,
    pub location: TextLocation,
    pub id: HirNodeId, // Unique ID for last-use analysis and borrow checking
}

pub type HirNodeId = usize;
pub type BlockId = usize;

#[derive(Debug, Clone)]
pub enum HirKind {
    // === Variable Operations ===
    /// Assign expression result to a variable
    /// The `is_mutable` flag indicates if this was a `~=` assignment
    Assign {
        name: InternedString,
        value: HirExpr,
        is_mutable: bool, // true for `~=`, false for `=`
    },

    // === Control Flow Blocks ===
    /// Conditional branch with explicit blocks
    If {
        condition: HirExpr,
        then_block: BlockId,
        else_block: Option<BlockId>,
    },

    /// Pattern matching
    Match {
        scrutinee: HirExpr,
        arms: Vec<HirMatchArm>,
        default_block: Option<BlockId>,
    },

    /// Loop with optional iteration binding
    Loop {
        label: BlockId, // Used by break/continue to reference this loop
        binding: Option<(InternedString, DataType)>, // Loop variable
        iterator: Option<HirExpr>, // None for infinite loops
        body: BlockId,
        index_binding: Option<InternedString>, // Optional index variable
    },

    Break {
        target: BlockId,
    },

    Continue {
        target: BlockId,
    },

    // === Function Calls ===
    /// Regular function call
    /// Results can be assigned via separate Assign nodes
    Call {
        target: InternedString,
        args: Vec<HirExpr>,
    },

    /// Host/builtin function call
    HostCall {
        target: InternedString,
        module: InternedString,
        import: InternedString,
        args: Vec<HirExpr>,
    },

    // === Error Handling ===
    /// Desugared error handling
    TryCall {
        call: Box<HirNode>,
        error_binding: Option<InternedString>,
        error_handler: BlockId,
        default_values: Option<Vec<HirExpr>>,
    },

    /// Option unwrapping with default
    OptionUnwrap {
        expr: HirExpr,
        default_value: Option<HirExpr>,
    },

    // === Returns ===
    /// Return with values (terminates block)
    Return(Vec<HirExpr>),

    /// Error return (terminates block)
    ReturnError(HirExpr),

    // === Resource Management ===
    /// Conditional drop inserted by HIR generation
    /// Will only drop if the value is owned at runtime
    PossibleDrop(InternedString),

    // === Templates ===
    /// Runtime template that becomes a function call
    RuntimeTemplateCall {
        template_fn: InternedString,
        captures: Vec<HirExpr>,
        id: Option<InternedString>,
    },

    /// Template function definition
    TemplateFn {
        name: InternedString,
        params: Vec<(InternedString, DataType)>,
        body: BlockId,
    },

    // === Function Definitions ===
    FunctionDef {
        name: InternedString,
        signature: FunctionSignature,
        body: BlockId,
    },

    // === Struct Definitions ===
    StructDef {
        name: InternedString,
        fields: Vec<Arg>,
    },

    // === Expression as Statement ===
    /// Expression evaluated for side effects only
    ExprStmt(HirExpr),
}

/// A basic block containing a sequence of HIR nodes
/// All blocks except the entry block are terminated by
/// a control flow node (Return, Break, Continue, or implicit fall-through)
#[derive(Debug, Clone)]
pub struct HirBlock {
    pub id: BlockId,
    pub nodes: Vec<HirNode>,
}

#[derive(Debug, Clone)]
pub struct HirExpr {
    pub kind: HirExprKind,
    pub data_type: DataType,
    pub location: TextLocation,
}

#[derive(Debug, Clone)]
pub enum HirExprKind {
    // === Literals ===
    Int(i64),
    Float(f64),
    Bool(bool),
    StringLiteral(InternedString), // Stack-allocated string slice
    HeapString(InternedString),    // Heap-allocated string (from runtime templates)
    Char(char),

    // === Variable Access ===
    /// Load variable value (creates shared reference by default)
    Var(InternedString),

    /// Field access on a variable
    Field {
        base: InternedString,
        field: InternedString,
    },

    /// Collection/array element access
    Index {
        base: InternedString,
        index: Box<HirExpr>,
    },

    /// Potential ownership transfer
    /// Marked during HIR generation based on last-use analysis hints
    /// Final ownership decision happens during borrow validation
    Move(InternedString),

    // === Binary Operations ===
    BinOp {
        left: Box<HirExpr>,
        op: BinOp,
        right: Box<HirExpr>,
    },

    /// Unary operation
    UnaryOp {
        op: UnaryOp,
        operand: Box<HirExpr>,
    },

    // === Function Calls ===
    Call {
        target: InternedString,
        args: Vec<HirExpr>,
    },

    /// Method call
    MethodCall {
        receiver: Box<HirExpr>,
        method: InternedString,
        args: Vec<HirExpr>,
    },

    // === Constructors ===
    StructConstruct {
        type_name: InternedString,
        fields: Vec<(InternedString, HirExpr)>,
    },

    Collection(Vec<HirExpr>),

    Range {
        start: Box<HirExpr>,
        end: Box<HirExpr>,
    },
}

#[derive(Debug, Clone)]
pub struct HirMatchArm {
    pub pattern: HirPattern,
    pub guard: Option<HirExpr>,
    pub body: BlockId,
}

#[derive(Debug, Clone)]
pub enum HirPattern {
    Literal(HirExpr),
    Range { start: HirExpr, end: HirExpr },
    Wildcard,
    // Future: variable bindings in patterns
}

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

/// The complete HIR module containing all blocks and metadata
#[derive(Debug, Clone)]
pub struct HirModule {
    pub blocks: Vec<HirBlock>,
    pub entry_block: BlockId,
    pub functions: Vec<HirNode>, // FunctionDef nodes
    pub structs: Vec<HirNode>,   // StructDef nodes
}
