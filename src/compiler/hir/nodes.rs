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
use crate::compiler::hir::place::Place;
use crate::compiler::interned_path::InternedPath;
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::InternedString;

#[derive(Debug, Clone)]
pub struct HirNode {
    pub kind: HirKind,
    pub location: TextLocation,
    pub scope: InternedPath,
    pub id: HirNodeId, // Unique ID for CFG construction and borrow checking
}

pub type HirNodeId = usize;
pub type ControlFlowId = usize;

#[derive(Debug, Clone)]
pub enum HirKind {
    // === Variable Bindings ===
    /// Assignment to a place (local variable, field, etc.)
    /// This covers both initial bindings and mutations
    Assign {
        place: Place,
        value: HirExpr,
    },

    /// Explicit borrow creation (shared or mutable)
    /// Records where borrow access is requested
    Borrow {
        place: Place,
        kind: BorrowKind,
        target: Place, // Where the borrow is stored
    },

    // === Control Flow ===
    /// Structured conditional with explicit blocks
    If {
        condition: Place, // Condition must be stored in a place first
        then_block: Vec<HirNode>,
        else_block: Option<Vec<HirNode>>,
    },

    /// Pattern matching with structured arms
    Match {
        scrutinee: Place, // Subject must be stored in a place first
        arms: Vec<HirMatchArm>,
        default: Option<Vec<HirNode>>,
    },

    /// Structured loop with explicit binding
    Loop {
        label: ControlFlowId, // ID label for breaks and continues to reference this loop
        binding: Option<(InternedString, DataType)>, // Loop variable binding
        iterator: Place,   // Iterator must be stored in a place first
        body: Vec<HirNode>,
        index_binding: Option<InternedString>, // Optional index binding
    },

    /// Loop control flow
    Break {
        target: ControlFlowId,
    },
    Continue {
        target: ControlFlowId,
    },

    // === Function Calls ===
    /// Regular function call with explicit argument places and return destinations
    Call {
        target: InternedString,
        args: Vec<Place>,    // Arguments must be stored in places first
        returns: Vec<Place>, // Return values stored to places
    },

    /// Host function call (builtin functions like io)
    HostCall {
        target: InternedString,
        module: InternedString,
        import: InternedString,
        args: Vec<Place>,    // Arguments must be stored in places first
        returns: Vec<Place>, // Return values stored to places
    },

    // === Error Handling (Desugared) ===
    TryCall {
        call: Box<HirNode>,
        error_binding: Option<InternedString>,
        error_handler: Vec<HirNode>,
        default_values: Option<Vec<HirExpr>>,
    },

    OptionUnwrap {
        expr: HirExpr,
        default_value: Option<HirExpr>,
    },

    // === Returns ===
    /// Return statement with values from places
    Return(Vec<Place>),

    /// Error return for `return!` syntax
    ReturnError(Place),

    // === Resource Management ===
    /// Drop operation (inserted by borrow checker after analysis)
    Drop(Place),

    // === Templates ===
    RuntimeTemplateCall {
        template_fn: InternedString,
        captures: Vec<HirExpr>,
        id: Option<InternedString>,
    },

    TemplateFn {
        name: InternedString,
        params: Vec<(InternedString, DataType)>,
        body: Vec<HirNode>,
    },

    // === Function Definitions ===
    FunctionDef {
        name: InternedString,
        signature: FunctionSignature,
        body: Vec<HirNode>,
    },

    // === Struct Definitions ===
    StructDef {
        name: InternedString,
        fields: Vec<Arg>,
    },

    // === Expressions as Statements ===
    /// Expression evaluated for side effects (result discarded)
    ExprStmt(Place), // Expression result must be stored in a place first
}

#[derive(Debug, Clone)]
pub struct HirExpr {
    pub kind: HirExprKind,
    pub data_type: DataType,
    #[allow(dead_code)]
    pub location: TextLocation,
}

#[derive(Debug, Clone)]
pub enum HirExprKind {
    // === Literals ===
    Int(i64),
    Float(f64),
    Bool(bool),
    StringLiteral(InternedString), // Includes compile-time folded templates (stack-allocated)
    HeapString(InternedString),    // Runtime template strings (heap-allocated)
    Char(char),

    // === Place Operations ===
    /// Load value from a place (immutable access)
    Load(Place),

    /// Create shared borrow of a place
    #[allow(dead_code)]
    SharedBorrow(Place),

    /// Create a mutable borrow of a place (exclusive access)
    #[allow(dead_code)]
    MutableBorrow(Place),

    /// Candidate move (potential ownership transfer, refined by the borrow checker)
    /// The BorrowId is attached during HIR generation for direct O(1) refinement
    CandidateMove(
        Place,
        Option<crate::compiler::borrow_checker::types::BorrowId>,
    ),

    // === Binary Operations ===
    /// Binary operation between two places (no nested expressions)
    BinOp {
        left: Place,
        op: BinOp,
        right: Place,
    },

    /// Unary operation on a place
    #[allow(dead_code)]
    UnaryOp {
        op: UnaryOp,
        operand: Place,
    },

    // === Function Calls ===
    /// Function call with arguments from places
    Call {
        target: InternedString,
        args: Vec<Place>,
    },

    /// Method call with receiver and arguments from places
    #[allow(dead_code)]
    MethodCall {
        receiver: Place,
        method: InternedString,
        args: Vec<Place>,
    },

    // === Constructors ===
    /// Struct construction with field values from places
    StructConstruct {
        type_name: InternedString,
        fields: Vec<(InternedString, Place)>,
    },

    /// Collection construction with elements from places
    Collection(Vec<Place>),

    /// Range construction with start and end from places
    Range {
        start: Place,
        end: Place,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum BorrowKind {
    #[allow(dead_code)]
    Shared, // Default: `x = y`
    #[allow(dead_code)]
    Mutable, // Explicit: `x ~= y` before move analysis
}

#[derive(Debug, Clone)]
pub struct HirMatchArm {
    pub pattern: HirPattern,
    pub guard: Option<HirExpr>,
    pub body: Vec<HirNode>,
}

#[derive(Debug, Clone)]
pub enum HirPattern {
    #[allow(dead_code)]
    Literal(HirExpr),
    #[allow(dead_code)]
    Range {
        start: HirExpr,
        end: HirExpr,
    },
    Wildcard,
    // Future: Binding(InternedString) for pattern matching with bindings
    // The AST has not yet implemented this
}

#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    Neg,
    #[allow(dead_code)]
    Not,
}
