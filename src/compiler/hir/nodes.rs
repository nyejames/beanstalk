//! HIR core node definitions
//!
//! This module defines the High-Level Intermediate Representation (HIR) for Beanstalk.
//! HIR is a structured, semantically rich IR designed for borrow checking, move analysis,
//! and preparing code for reliable lowering to multiple backends.
//!
//! ## Memory Model Strategy
//!
//! Beanstalk uses a **fallback GC approach**:
//! - **Baseline semantics**: All heap values are GC-managed (correct by default)
//! - **Progressive optimization**: Static analysis identifies values eligible for deterministic management
//! - **Backend flexibility**:
//!   - JS backend: Pure GC semantics (ignore ownership annotations)
//!   - Wasm backend: Hybrid GC + ownership (initially GC-heavy, progressively optimized)
//!
//! HIR is designed to support both models:
//! - Ownership annotations are **advisory hints** for optimization, not semantic requirements
//! - All programs are correct under pure GC interpretation
//! - Static analysis strengthens guarantees incrementally without changing HIR structure
//!
//! Key design principles:
//! - Place-based memory model for precise borrow tracking (when enabled)
//! - No nested control flow; expressions may nest, but evaluation order is explicit
//! - Borrow intent recorded, an ownership outcome determined later (or ignored entirely in GC-only backends)
//! - Language-shaped, not Wasm-shaped (deferred to LIR)

use crate::compiler::datatypes::DataType;
use crate::compiler::host_functions::registry::CallTarget;
use crate::compiler::parsers::ast_nodes::Var;
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::InternedString;

/// The complete HIR module containing all blocks and metadata
#[derive(Debug, Clone)]
pub struct HirModule {
    pub blocks: Vec<HirBlock>,
    pub entry_block: BlockId,
    pub functions: Vec<HirNode>, // FunctionDef nodes
    pub structs: Vec<HirNode>,   // StructDef nodes
}

/// A basic block containing a sequence of HIR nodes
/// All blocks except the entry block are terminated by an HirTerminator
#[derive(Debug, Clone)]
pub struct HirBlock {
    pub id: BlockId,
    pub params: Vec<InternedString>,
    pub nodes: Vec<HirNode>,
}

#[derive(Debug, Clone)]
pub struct HirNode {
    pub kind: HirKind,
    pub location: TextLocation,
    pub id: HirNodeId, // Unique ID for last-use analysis and borrow checking
}

pub type HirNodeId = usize;
pub type BlockId = usize;

#[derive(Debug, Clone)]
pub enum HirPlace {
    Var(InternedString),
    Field {
        base: Box<HirPlace>,
        field: InternedString,
    },
    Index {
        base: Box<HirPlace>,
        index: Box<HirExpr>,
    },
}

#[derive(Debug, Clone)]
pub enum HirKind {
    Stmt(HirStmt),
    Terminator(HirTerminator),
}

/// Memory management classification for values
///
/// This is purely for optimization - it does not affect correctness.
/// All values work correctly under GC regardless of classification.
#[derive(Debug, Clone)]
enum MemoryClass {
    /// Default: Always GC-managed
    /// - JS backend: All values start and remain here
    /// - Wasm backend (early): All values start here
    Unknown,

    /// Optimization hint: May be eligible for deterministic management
    /// - JS backend: Ignored (remains GC-managed)
    /// - Wasm backend (later): Can use ownership elision if borrow checker validates
    ///
    /// This classification does not guarantee deterministic management - it's a hint
    /// that static analysis *might* be able to prove safety for non-GC lowering.
    Eligible,
}

#[derive(Debug, Clone)]
pub enum HirStmt {
    // === Variable Operations ===
    /// Assign expression result to a variable
    /// The `is_mutable` flag indicates if this was a `~=` assignment
    ///
    /// **GC semantics**: Creates/updates a GC-managed binding
    /// **Ownership semantics** (when enabled): May transfer ownership based on last-use
    Assign {
        target: HirPlace,
        value: HirExpr,
        is_mutable: bool,
    },

    // === Function Calls ===
    /// Regular function call
    /// Results can be assigned via separate Assign nodes
    Call {
        target: CallTarget, // Could be a host function call also
        args: Vec<HirExpr>,
    },

    // === Resource Management ===
    /// Conditional drop inserted by HIR generation
    ///
    /// **Semantics by backend**:
    /// - JS backend: No-op (GC handles cleanup)
    /// - Wasm GC backend (early): No-op (GC handles cleanup)
    /// - Wasm ownership backend (later): Conditional free based on runtime ownership flag
    ///
    /// This node is always safe to insert - it's an optimization hint that may become
    /// executable code in backends with ownership elision.
    PossibleDrop(HirPlace),

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
        fields: Vec<Var>,
    },

    // === Expression as Statement ===
    /// Expression evaluated for side effects only
    ExprStmt(HirExpr),
}

#[derive(Debug, Clone)]
pub enum HirTerminator {
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

    // === Returns ===
    /// Return with values (terminates block)
    Return(Vec<HirExpr>),

    /// Error return (terminates block)
    ReturnError(HirExpr),

    /// Hard break - always terminates program
    Panic {
        message: Option<HirExpr>,
    },
}

#[derive(Debug, Clone)]
pub struct HirExpr {
    pub kind: HirExprKind,
    pub location: TextLocation,
}

#[derive(Debug, Clone)]
pub enum HirExprKind {
    // === Literals ===
    /// All literal values are GC-managed by default
    /// Optimization: Small literals (word-sized types) may be stack-allocated in Wasm backend
    Int(i64),
    Float(f64),
    Bool(bool),
    StringLiteral(InternedString), // Stack-allocated string slice (immutable, from source)
    HeapString(InternedString),    // Heap-allocated string (mutable, from runtime templates)
    Char(char),

    // === Variable Access ===
    /// Load variable value
    ///
    /// **GC semantics**: Always creates a shared reference to GC-managed data
    /// **Ownership semantics** (when enabled): Creates shared reference; exclusive access
    /// rules enforced by borrow checker
    Load(HirPlace),

    /// Field access on a variable
    Field {
        base: InternedString,
        field: InternedString,
    },

    /// Potential ownership transfer (marked by last-use analysis)
    ///
    /// **GC semantics**: Same as Load (GC handles aliasing)
    /// **Ownership semantics** (when enabled): May consume ownership if:
    ///   - Borrow checker validates exclusive access
    ///   - This is the last use in control flow
    ///   - Runtime ownership flag permits it
    ///
    /// This is an **advisory annotation** - the distinction between Move and Load
    /// is purely for optimization and does not affect program correctness under GC.
    Move(HirPlace),

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
        target: CallTarget,
        args: Vec<HirExpr>,
    },

    /// Method call
    MethodCall {
        receiver: Box<HirExpr>,
        method: InternedString,
        args: Vec<HirExpr>,
    },

    // === Constructors ===
    /// All composite types are GC-managed by default
    /// Ownership semantics (when enabled) may optimize allocation patterns
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
