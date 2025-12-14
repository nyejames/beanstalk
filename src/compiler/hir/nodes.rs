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
//! - Borrow intent, not ownership outcome (determined by borrow checker)
//! - Language-shaped, not Wasm-shaped (deferred to LIR)

use crate::compiler::datatypes::DataType;
use crate::compiler::hir::place::Place;
use crate::compiler::interned_path::InternedPath;
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::InternedString;
use std::fmt::{Display, Formatter, Result as FmtResult};
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
    pub id: HirNodeId, // Unique ID for CFG construction and borrow checking
}

pub type HirNodeId = usize;

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
        binding: Option<(InternedString, DataType)>, // Loop variable binding
        iterator: Place, // Iterator must be stored in a place first
        body: Vec<HirNode>,
        index_binding: Option<InternedString>, // Optional index binding
    },

    /// Loop control flow
    Break,
    Continue,

    // === Function Calls ===
    /// Regular function call with explicit argument places and return destinations
    Call {
        target: InternedString,
        args: Vec<Place>, // Arguments must be stored in places first
        returns: Vec<Place>, // Return values stored to places
    },

    /// Host function call (builtin functions like io)
    HostCall {
        target: InternedString,
        module: InternedString,
        import: InternedString,
        args: Vec<Place>, // Arguments must be stored in places first
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
    /// Load value from a place (immutable access)
    Load(Place),
    
    /// Create shared borrow of a place
    SharedBorrow(Place),
    
    /// Create mutable borrow of a place (exclusive access)
    MutableBorrow(Place),
    
    /// Candidate move (potential ownership transfer, refined by borrow checker)
    CandidateMove(Place),

    // === Binary Operations ===
    /// Binary operation between two places (no nested expressions)
    BinOp {
        left: Place,
        op: BinOp,
        right: Place,
    },

    /// Unary operation on a place
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
    Root,
    Exponent,
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Not,
}

// === Display Implementations for HIR Debugging ===

impl Display for HirModule {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        if let Some(path) = &self.source_path {
            writeln!(f, "HIR Module: {}", path.display())?;
        } else {
            writeln!(f, "HIR Module: <unknown>")?;
        }
        
        for (i, function) in self.functions.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{}", function)?;
        }
        
        Ok(())
    }
}

impl Display for HirNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "HIR Node #{} ({}:{}:{:?}): ", 
               self.id, 
               self.location.start_pos.line_number, 
               self.location.start_pos.char_column,
               self.scope)?;
        
        match &self.kind {
            HirKind::Assign { place, value } => {
                writeln!(f, "Assign")?;
                writeln!(f, "  Place: {}", place)?;
                write!(f, "  Value: {}", value)?;
            }
            
            HirKind::Borrow { place, kind, target } => {
                writeln!(f, "Borrow ({:?})", kind)?;
                writeln!(f, "  Place: {}", place)?;
                write!(f, "  Target: {}", target)?;
            }
            
            HirKind::If { condition, then_block, else_block } => {
                writeln!(f, "If")?;
                writeln!(f, "  Condition: {}", condition)?;
                writeln!(f, "  Then:")?;
                for node in then_block {
                    write!(f, "{}", indent_lines(&node.to_string(), 4))?;
                }
                if let Some(else_nodes) = else_block {
                    writeln!(f, "  Else:")?;
                    for node in else_nodes {
                        write!(f, "{}", indent_lines(&node.to_string(), 4))?;
                    }
                }
            }
            
            HirKind::Match { scrutinee, arms, default } => {
                writeln!(f, "Match")?;
                writeln!(f, "  Scrutinee: {}", scrutinee)?;
                writeln!(f, "  Arms:")?;
                for (i, arm) in arms.iter().enumerate() {
                    writeln!(f, "    Arm {}:", i)?;
                    writeln!(f, "      Pattern: {:?}", arm.pattern)?;
                    if let Some(guard) = &arm.guard {
                        writeln!(f, "      Guard: {}", guard)?;
                    }
                    writeln!(f, "      Body:")?;
                    for node in &arm.body {
                        write!(f, "{}", indent_lines(&node.to_string(), 8))?;
                    }
                }
                if let Some(default_nodes) = default {
                    writeln!(f, "  Default:")?;
                    for node in default_nodes {
                        write!(f, "{}", indent_lines(&node.to_string(), 4))?;
                    }
                }
            }
            
            HirKind::Loop { binding, iterator, body, index_binding } => {
                writeln!(f, "Loop")?;
                if let Some((name, data_type)) = binding {
                    writeln!(f, "  Binding: {} : {:?}", name, data_type)?;
                }
                if let Some(index_name) = index_binding {
                    writeln!(f, "  Index Binding: {}", index_name)?;
                }
                writeln!(f, "  Iterator: {}", iterator)?;
                writeln!(f, "  Body:")?;
                for node in body {
                    write!(f, "{}", indent_lines(&node.to_string(), 4))?;
                }
            }
            
            HirKind::Break => write!(f, "Break")?,
            HirKind::Continue => write!(f, "Continue")?,
            
            HirKind::Call { target, args, returns } => {
                writeln!(f, "Call")?;
                writeln!(f, "  Target: {}", target)?;
                writeln!(f, "  Args: [{}]", format_place_list(args))?;
                write!(f, "  Returns: [{}]", format_place_list(returns))?;
            }
            
            HirKind::HostCall { target, module, import, args, returns } => {
                writeln!(f, "HostCall")?;
                writeln!(f, "  Target: {}", target)?;
                writeln!(f, "  Module: {}", module)?;
                writeln!(f, "  Import: {}", import)?;
                writeln!(f, "  Args: [{}]", format_place_list(args))?;
                write!(f, "  Returns: [{}]", format_place_list(returns))?;
            }
            
            HirKind::TryCall { call, error_binding, error_handler, default_values } => {
                writeln!(f, "TryCall")?;
                writeln!(f, "  Call:")?;
                write!(f, "{}", indent_lines(&call.to_string(), 4))?;
                if let Some(binding) = error_binding {
                    writeln!(f, "  Error Binding: {}", binding)?;
                }
                writeln!(f, "  Error Handler:")?;
                for node in error_handler {
                    write!(f, "{}", indent_lines(&node.to_string(), 4))?;
                }
                if let Some(defaults) = default_values {
                    writeln!(f, "  Default Values:")?;
                    for (i, default) in defaults.iter().enumerate() {
                        writeln!(f, "    {}: {}", i, default)?;
                    }
                }
            }
            
            HirKind::OptionUnwrap { expr, default_value } => {
                writeln!(f, "OptionUnwrap")?;
                writeln!(f, "  Expr: {}", expr)?;
                if let Some(default) = default_value {
                    write!(f, "  Default: {}", default)?;
                } else {
                    write!(f, "  Default: None")?;
                }
            }
            
            HirKind::Return(places) => {
                write!(f, "Return [{}]", format_place_list(places))?;
            }
            
            HirKind::ReturnError(place) => {
                write!(f, "ReturnError {}", place)?;
            }
            
            HirKind::Drop(place) => {
                write!(f, "Drop {}", place)?;
            }
            
            HirKind::RuntimeTemplateCall { template_fn, captures, id } => {
                writeln!(f, "RuntimeTemplateCall")?;
                writeln!(f, "  Template: {}", template_fn)?;
                if let Some(template_id) = id {
                    writeln!(f, "  ID: {}", template_id)?;
                }
                writeln!(f, "  Captures:")?;
                for (i, capture) in captures.iter().enumerate() {
                    writeln!(f, "    {}: {}", i, capture)?;
                }
            }
            
            HirKind::TemplateFn { name, params, body } => {
                writeln!(f, "TemplateFn {}", name)?;
                writeln!(f, "  Params:")?;
                for (param_name, param_type) in params {
                    writeln!(f, "    {} : {:?}", param_name, param_type)?;
                }
                writeln!(f, "  Body:")?;
                for node in body {
                    write!(f, "{}", indent_lines(&node.to_string(), 4))?;
                }
            }
            
            HirKind::FunctionDef { name, signature, body } => {
                writeln!(f, "FunctionDef {}", name)?;
                writeln!(f, "  Signature: {:?}", signature)?;
                writeln!(f, "  Body:")?;
                for node in body {
                    write!(f, "{}", indent_lines(&node.to_string(), 4))?;
                }
            }
            
            HirKind::StructDef { name, fields } => {
                writeln!(f, "StructDef {}", name)?;
                writeln!(f, "  Fields:")?;
                for field in fields {
                    writeln!(f, "    {:?}", field)?;
                }
            }
            
            HirKind::ExprStmt(place) => {
                write!(f, "ExprStmt {}", place)?;
            }
        }
        
        Ok(())
    }
}

impl Display for HirExpr {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{} : {:?}", self.kind, self.data_type)
    }
}

impl Display for HirExprKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            HirExprKind::Int(value) => write!(f, "{}", value),
            HirExprKind::Float(value) => write!(f, "{}", value),
            HirExprKind::Bool(value) => write!(f, "{}", value),
            HirExprKind::StringLiteral(value) => write!(f, "\"{}\"", value),
            HirExprKind::Char(value) => write!(f, "'{}'", value),
            
            HirExprKind::Load(place) => write!(f, "Load({})", place),
            HirExprKind::SharedBorrow(place) => write!(f, "SharedBorrow({})", place),
            HirExprKind::MutableBorrow(place) => write!(f, "MutableBorrow({})", place),
            HirExprKind::CandidateMove(place) => write!(f, "CandidateMove({})", place),
            
            HirExprKind::BinOp { left, op, right } => {
                write!(f, "({} {} {})", left, op, right)
            }
            
            HirExprKind::UnaryOp { op, operand } => {
                write!(f, "({} {})", op, operand)
            }
            
            HirExprKind::Call { target, args } => {
                write!(f, "{}({})", target, format_place_list(args))
            }
            
            HirExprKind::MethodCall { receiver, method, args } => {
                write!(f, "{}.{}({})", receiver, method, format_place_list(args))
            }
            
            HirExprKind::StructConstruct { type_name, fields } => {
                write!(f, "{} {{ ", type_name)?;
                for (i, (field_name, place)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", field_name, place)?;
                }
                write!(f, " }}")
            }
            
            HirExprKind::Collection(places) => {
                write!(f, "[{}]", format_place_list(places))
            }
            
            HirExprKind::Range { start, end } => {
                write!(f, "{}..{}", start, end)
            }
        }
    }
}

impl Display for BinOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let op_str = match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
            BinOp::Root => "root",
            BinOp::Exponent => "**",
        };
        write!(f, "{}", op_str)
    }
}

impl Display for UnaryOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let op_str = match self {
            UnaryOp::Neg => "-",
            UnaryOp::Not => "!",
        };
        write!(f, "{}", op_str)
    }
}

// Helper functions for formatting

fn format_place_list(places: &[Place]) -> String {
    places.iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn indent_lines(text: &str, spaces: usize) -> String {
    let indent = " ".repeat(spaces);
    text.lines()
        .map(|line| {
            if line.trim().is_empty() {
                line.to_string()
            } else {
                format!("{}{}", indent, line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}
