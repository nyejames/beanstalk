//! HIR core node definitions (scaffold)
//!
//! This module defines the minimal data structures for the High-Level IR (HIR)
//! that other stages (borrow checker, lowering) can reference. These are
//! intentionally lightweight placeholders and will evolve as the compiler is
//! implemented.

use crate::compiler::datatypes::DataType;
use crate::compiler::hir::place::Place;
use crate::compiler::interned_path::InternedPath;
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::InternedString;
use std::path::PathBuf;

/// A complete HIR module for a single source file or compilation unit.
#[derive(Debug, Default, Clone)]
pub struct HirModule {
    pub source_path: Option<PathBuf>,
    pub functions: Vec<HirNode>,
}

#[derive(Debug, Clone)]
pub struct HirNode {
    pub kind: HirKind,
    pub location: TextLocation,
    pub scope: InternedPath,

    // Metadata for borrow checker
    pub id: HirNodeId, // Unique ID for CFG construction
}

pub type HirNodeId = usize;

#[derive(Debug, Clone)]
pub enum HirKind {
    // === Variable Bindings ===
    Let {
        place: Place,
        value: HirExpr,
    },

    LetMulti {
        places: Vec<Place>,
        value: HirExpr, // Must be multi-return call
    },

    // Store to existing place
    Store {
        place: Place,
        value: HirExpr,
    },

    // === Control Flow ===
    If {
        condition: HirExpr,
        then_block: Vec<HirNode>,
        else_block: Option<Vec<HirNode>>,
    },

    Match {
        scrutinee: HirExpr,
        arms: Vec<HirMatchArm>,
        default: Option<Vec<HirNode>>,
    },

    Loop {
        // For `loop item in collection:`
        binding: Option<(InternedString, DataType)>,
        iterator: HirExpr,
        body: Vec<HirNode>,
        // Optional index binding for `loop item, index in collection:`
        index_binding: Option<InternedString>,
    },

    Break,
    Continue,

    // === Function Calls ===
    Call {
        target: InternedString,
        args: Vec<HirExpr>,
        returns: Vec<DataType>,
    },

    HostCall {
        target: InternedString,
        module: InternedString,
        import: InternedString,
        args: Vec<HirExpr>,
        returns: Vec<DataType>,
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
    Return(Vec<HirExpr>),
    ReturnError(HirExpr), // For `return!` syntax

    // === Resource Management ===
    // Inserted by borrow checker after analysis
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
    // For expressions evaluated for side effects
    Expr(HirExpr),
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
    StringLiteral(InternedString), // Includes compile-time folded templates
    Char(char),

    // === Place Operations ===
    Load(Place),               // Read from place (immutable)
    Borrow(Place, BorrowKind), // Borrow place (shared or mutable)
    CandidateMove(Place),      // Potential move (refined by borrow checker)

    // === Operations ===
    BinOp {
        left: Box<HirExpr>,
        op: BinOp,
        right: Box<HirExpr>,
    },

    UnaryOp {
        op: UnaryOp,
        expr: Box<HirExpr>,
    },

    // === Calls ===
    Call {
        target: InternedString,
        args: Vec<HirExpr>,
    },

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

    // === Special ===
    Range {
        start: Box<HirExpr>,
        end: Box<HirExpr>,
    },

    // Function values
    Function {
        signature: FunctionSignature,
        body: Vec<HirNode>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum BorrowKind {
    Shared,  // Default: `x = y`
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
    Literal(HirExpr),
    Range { start: HirExpr, end: HirExpr },
    Wildcard,
    // Future: Binding(InternedString) for pattern matching with bindings
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
    // Add others as needed
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Not,
}
