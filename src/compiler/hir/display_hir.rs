use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::{InternedString, StringTable};
use std::fmt::{Display, Formatter, Result as FmtResult};

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
    pub params: Vec<InternedString>,
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

// === Display Implementations ===

/// Helper function to indent lines for nested display
fn indent_lines(text: &str, spaces: usize) -> String {
    let indent = " ".repeat(spaces);
    text.lines()
        .map(|line| format!("{}{}\n", indent, line))
        .collect()
}

impl HirNode {
    /// Display HIR node with resolved string IDs for debugging
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        let mut result = format!(
            "HIR Node #{} ({}:{}): ",
            self.id, self.location.start_pos.line_number, self.location.start_pos.char_column,
        );

        match &self.kind {
            HirKind::Assign {
                name,
                value,
                is_mutable,
            } => {
                result.push_str(&format!(
                    "Assign{}\n",
                    if *is_mutable { " (mutable)" } else { "" }
                ));
                result.push_str(&format!("  Var: {}\n", string_table.resolve(*name)));
                result.push_str(&format!(
                    "  Value: {}",
                    value.display_with_table(string_table)
                ));
            }

            HirKind::If {
                condition,
                then_block,
                else_block,
            } => {
                result.push_str("If\n");
                result.push_str(&format!(
                    "  Condition: {}\n",
                    condition.display_with_table(string_table)
                ));
                result.push_str(&format!("  Then: Block #{}\n", then_block));
                if let Some(else_id) = else_block {
                    result.push_str(&format!("  Else: Block #{}", else_id));
                }
            }

            HirKind::Match {
                scrutinee,
                arms,
                default_block,
            } => {
                result.push_str("Match\n");
                result.push_str(&format!(
                    "  Scrutinee: {}\n",
                    scrutinee.display_with_table(string_table)
                ));
                result.push_str("  Arms:\n");
                for (i, arm) in arms.iter().enumerate() {
                    result.push_str(&format!("    Arm {}:\n", i));
                    result.push_str(&format!(
                        "      Pattern: {}\n",
                        arm.pattern.display_with_table(string_table)
                    ));
                    if let Some(guard) = &arm.guard {
                        result.push_str(&format!(
                            "      Guard: {}\n",
                            guard.display_with_table(string_table)
                        ));
                    }
                    result.push_str(&format!("      Body: Block #{}\n", arm.body));
                }
                if let Some(default_id) = default_block {
                    result.push_str(&format!("  Default: Block #{}", default_id));
                }
            }

            HirKind::Loop {
                label,
                binding,
                iterator,
                body,
                index_binding,
            } => {
                result.push_str(&format!("Loop (label: {})\n", label));
                if let Some((name, data_type)) = binding {
                    result.push_str(&format!(
                        "  Binding: {} : {:?}\n",
                        string_table.resolve(*name),
                        data_type
                    ));
                }
                if let Some(index_name) = index_binding {
                    result.push_str(&format!(
                        "  Index Binding: {}\n",
                        string_table.resolve(*index_name)
                    ));
                }
                if let Some(iter_expr) = iterator {
                    result.push_str(&format!(
                        "  Iterator: {}\n",
                        iter_expr.display_with_table(string_table)
                    ));
                }
                result.push_str(&format!("  Body: Block #{}", body));
            }

            HirKind::Break { target } => {
                result.push_str(&format!("Break (target: Block #{})", target));
            }

            HirKind::Continue { target } => {
                result.push_str(&format!("Continue (target: Block #{})", target));
            }

            HirKind::Call { target, args } => {
                result.push_str("Call\n");
                result.push_str(&format!("  Target: {}\n", string_table.resolve(*target)));
                result.push_str("  Args:\n");
                for (i, arg) in args.iter().enumerate() {
                    result.push_str(&format!(
                        "    {}: {}\n",
                        i,
                        arg.display_with_table(string_table)
                    ));
                }
            }

            HirKind::HostCall {
                target,
                module,
                import,
                args,
            } => {
                result.push_str("HostCall\n");
                result.push_str(&format!("  Target: {}\n", string_table.resolve(*target)));
                result.push_str(&format!("  Module: {}\n", string_table.resolve(*module)));
                result.push_str(&format!("  Import: {}\n", string_table.resolve(*import)));
                result.push_str("  Args:\n");
                for (i, arg) in args.iter().enumerate() {
                    result.push_str(&format!(
                        "    {}: {}\n",
                        i,
                        arg.display_with_table(string_table)
                    ));
                }
            }

            HirKind::TryCall {
                call,
                error_binding,
                error_handler,
                default_values,
            } => {
                result.push_str("TryCall\n");
                result.push_str("  Call:\n");
                result.push_str(&indent_lines(&call.display_with_table(string_table), 4));
                if let Some(binding) = error_binding {
                    result.push_str(&format!(
                        "  Error Binding: {}\n",
                        string_table.resolve(*binding)
                    ));
                }
                result.push_str(&format!("  Error Handler: Block #{}\n", error_handler));
                if let Some(defaults) = default_values {
                    result.push_str("  Default Values:\n");
                    for (i, default) in defaults.iter().enumerate() {
                        result.push_str(&format!(
                            "    {}: {}\n",
                            i,
                            default.display_with_table(string_table)
                        ));
                    }
                }
            }

            HirKind::OptionUnwrap {
                expr,
                default_value,
            } => {
                result.push_str("OptionUnwrap\n");
                result.push_str(&format!(
                    "  Expr: {}\n",
                    expr.display_with_table(string_table)
                ));
                if let Some(default) = default_value {
                    result.push_str(&format!(
                        "  Default: {}",
                        default.display_with_table(string_table)
                    ));
                } else {
                    result.push_str("  Default: None");
                }
            }

            HirKind::Return(exprs) => {
                result.push_str("Return\n");
                for (i, expr) in exprs.iter().enumerate() {
                    result.push_str(&format!(
                        "  {}: {}\n",
                        i,
                        expr.display_with_table(string_table)
                    ));
                }
            }

            HirKind::ReturnError(expr) => {
                result.push_str(&format!(
                    "ReturnError {}",
                    expr.display_with_table(string_table)
                ));
            }

            HirKind::PossibleDrop(name) => {
                result.push_str(&format!("PossibleDrop {}", string_table.resolve(*name)));
            }

            HirKind::RuntimeTemplateCall {
                template_fn,
                captures,
                id,
            } => {
                result.push_str("RuntimeTemplateCall\n");
                result.push_str(&format!(
                    "  Template: {}\n",
                    string_table.resolve(*template_fn)
                ));
                if let Some(template_id) = id {
                    result.push_str(&format!("  ID: {}\n", string_table.resolve(*template_id)));
                }
                result.push_str("  Captures:\n");
                for (i, capture) in captures.iter().enumerate() {
                    result.push_str(&format!(
                        "    {}: {}\n",
                        i,
                        capture.display_with_table(string_table)
                    ));
                }
            }

            HirKind::TemplateFn { name, params, body } => {
                result.push_str(&format!("TemplateFn {}\n", string_table.resolve(*name)));
                result.push_str("  Params:\n");
                for (param_name, param_type) in params {
                    result.push_str(&format!(
                        "    {} : {:?}\n",
                        string_table.resolve(*param_name),
                        param_type
                    ));
                }
                result.push_str(&format!("  Body: Block #{}", body));
            }

            HirKind::FunctionDef {
                name,
                signature,
                body,
            } => {
                result.push_str(&format!("FunctionDef {}\n", string_table.resolve(*name)));
                result.push_str(&format!("  Signature: {:?}\n", signature));
                result.push_str(&format!("  Body: Block #{}", body));
            }

            HirKind::StructDef { name, fields } => {
                result.push_str(&format!("StructDef {}\n", string_table.resolve(*name)));
                result.push_str("  Fields:\n");
                for field in fields {
                    result.push_str(&format!("    {:?}\n", field));
                }
            }

            HirKind::ExprStmt(expr) => {
                result.push_str(&format!(
                    "ExprStmt {}",
                    expr.display_with_table(string_table)
                ));
            }
        }

        result
    }
}

impl Display for HirNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(
            f,
            "HIR Node #{} ({}:{}): ",
            self.id, self.location.start_pos.line_number, self.location.start_pos.char_column,
        )?;

        // Note: This Display implementation shows StringID placeholders.
        // Use display_with_table() for debugging with resolved strings.
        match &self.kind {
            HirKind::Assign {
                name,
                value,
                is_mutable,
            } => {
                writeln!(f, "Assign{}", if *is_mutable { " (mutable)" } else { "" })?;
                writeln!(f, "  Var: StringID({})", name)?;
                write!(f, "  Value: {}", value)?;
            }

            HirKind::If {
                condition,
                then_block,
                else_block,
            } => {
                writeln!(f, "If")?;
                writeln!(f, "  Condition: {}", condition)?;
                writeln!(f, "  Then: Block #{}", then_block)?;
                if let Some(else_id) = else_block {
                    write!(f, "  Else: Block #{}", else_id)?;
                }
            }

            HirKind::Match {
                scrutinee,
                arms,
                default_block,
            } => {
                writeln!(f, "Match")?;
                writeln!(f, "  Scrutinee: {}", scrutinee)?;
                writeln!(f, "  Arms:")?;
                for (i, arm) in arms.iter().enumerate() {
                    writeln!(f, "    Arm {}:", i)?;
                    writeln!(f, "      Pattern: {}", arm.pattern)?;
                    if let Some(guard) = &arm.guard {
                        writeln!(f, "      Guard: {}", guard)?;
                    }
                    writeln!(f, "      Body: Block #{}", arm.body)?;
                }
                if let Some(default_id) = default_block {
                    write!(f, "  Default: Block #{}", default_id)?;
                }
            }

            HirKind::Loop {
                label,
                binding,
                iterator,
                body,
                index_binding,
            } => {
                writeln!(f, "Loop (label: {})", label)?;
                if let Some((name, data_type)) = binding {
                    writeln!(f, "  Binding: StringID({}) : {:?}", name.0, data_type)?;
                }
                if let Some(index_name) = index_binding {
                    writeln!(f, "  Index Binding: StringID({})", index_name.0)?;
                }
                if let Some(iter_expr) = iterator {
                    writeln!(f, "  Iterator: {}", iter_expr)?;
                }
                write!(f, "  Body: Block #{}", body)?;
            }

            HirKind::Break { target } => {
                write!(f, "Break (target: Block #{})", target)?;
            }

            HirKind::Continue { target } => {
                write!(f, "Continue (target: Block #{})", target)?;
            }

            HirKind::Call { target, args } => {
                writeln!(f, "Call")?;
                writeln!(f, "  Target: StringID({})", target.0)?;
                writeln!(f, "  Args:")?;
                for (i, arg) in args.iter().enumerate() {
                    writeln!(f, "    {}: {}", i, arg)?;
                }
            }

            HirKind::HostCall {
                target,
                module,
                import,
                args,
            } => {
                writeln!(f, "HostCall")?;
                writeln!(f, "  Target: StringID({})", target.0)?;
                writeln!(f, "  Module: StringID({})", module.0)?;
                writeln!(f, "  Import: StringID({})", import.0)?;
                writeln!(f, "  Args:")?;
                for (i, arg) in args.iter().enumerate() {
                    writeln!(f, "    {}: {}", i, arg)?;
                }
            }

            HirKind::TryCall {
                call,
                error_binding,
                error_handler,
                default_values,
            } => {
                writeln!(f, "TryCall")?;
                writeln!(f, "  Call:")?;
                write!(f, "{}", indent_lines(&call.to_string(), 4))?;
                if let Some(binding) = error_binding {
                    writeln!(f, "  Error Binding: StringID({})", binding.0)?;
                }
                writeln!(f, "  Error Handler: Block #{}", error_handler)?;
                if let Some(defaults) = default_values {
                    writeln!(f, "  Default Values:")?;
                    for (i, default) in defaults.iter().enumerate() {
                        writeln!(f, "    {}: {}", i, default)?;
                    }
                }
            }

            HirKind::OptionUnwrap {
                expr,
                default_value,
            } => {
                writeln!(f, "OptionUnwrap")?;
                writeln!(f, "  Expr: {}", expr)?;
                if let Some(default) = default_value {
                    write!(f, "  Default: {}", default)?;
                } else {
                    write!(f, "  Default: None")?;
                }
            }

            HirKind::Return(exprs) => {
                writeln!(f, "Return")?;
                for (i, expr) in exprs.iter().enumerate() {
                    writeln!(f, "  {}: {}", i, expr)?;
                }
            }

            HirKind::ReturnError(expr) => {
                write!(f, "ReturnError {}", expr)?;
            }

            HirKind::PossibleDrop(name) => {
                write!(f, "PossibleDrop StringID({})", name.0)?;
            }

            HirKind::RuntimeTemplateCall {
                template_fn,
                captures,
                id,
            } => {
                writeln!(f, "RuntimeTemplateCall")?;
                writeln!(f, "  Template: StringID({})", template_fn.0)?;
                if let Some(template_id) = id {
                    writeln!(f, "  ID: StringID({})", template_id.0)?;
                }
                writeln!(f, "  Captures:")?;
                for (i, capture) in captures.iter().enumerate() {
                    writeln!(f, "    {}: {}", i, capture)?;
                }
            }

            HirKind::TemplateFn { name, params, body } => {
                writeln!(f, "TemplateFn StringID({})", name.0)?;
                writeln!(f, "  Params:")?;
                for (param_name, param_type) in params {
                    writeln!(f, "    StringID({}) : {:?}", param_name.0, param_type)?;
                }
                write!(f, "  Body: Block #{}", body)?;
            }

            HirKind::FunctionDef {
                name,
                signature,
                body,
            } => {
                writeln!(f, "FunctionDef StringID({})", name.0)?;
                writeln!(f, "  Signature: {:?}", signature)?;
                write!(f, "  Body: Block #{}", body)?;
            }

            HirKind::StructDef { name, fields } => {
                writeln!(f, "StructDef StringID({})", name.0)?;
                writeln!(f, "  Fields:")?;
                for field in fields {
                    writeln!(f, "    {:?}", field)?;
                }
            }

            HirKind::ExprStmt(expr) => {
                write!(f, "ExprStmt {}", expr)?;
            }
        }

        Ok(())
    }
}

impl HirExpr {
    /// Display HIR expression with resolved string IDs for debugging
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        format!(
            "{} : {:?}",
            self.kind.display_with_table(string_table),
            self.data_type
        )
    }
}

impl Display for HirExpr {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        // Note: This Display implementation shows StringID placeholders.
        // Use display_with_table() for debugging with resolved strings.
        write!(f, "{} : {:?}", self.kind, self.data_type)
    }
}

impl HirExprKind {
    /// Display HIR expression kind with resolved string IDs for debugging
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        match self {
            HirExprKind::Int(value) => value.to_string(),
            HirExprKind::Float(value) => value.to_string(),
            HirExprKind::Bool(value) => value.to_string(),
            HirExprKind::StringLiteral(value) => format!("\"{}\"", string_table.resolve(*value)),
            HirExprKind::HeapString(value) => {
                format!("heap_string(\"{}\")", string_table.resolve(*value))
            }
            HirExprKind::Char(value) => format!("'{}'", value),

            HirExprKind::Var(name) => string_table.resolve(*name).to_string(),

            HirExprKind::Field { base, field } => {
                format!(
                    "{}.{}",
                    string_table.resolve(*base),
                    string_table.resolve(*field)
                )
            }

            HirExprKind::Index { base, index } => {
                format!(
                    "{}.get({})",
                    string_table.resolve(*base),
                    index.display_with_table(string_table)
                )
            }

            HirExprKind::Move(name) => {
                format!("move({})", string_table.resolve(*name))
            }

            HirExprKind::BinOp { left, op, right } => {
                format!(
                    "({} {:?} {})",
                    left.display_with_table(string_table),
                    op,
                    right.display_with_table(string_table)
                )
            }

            HirExprKind::UnaryOp { op, operand } => {
                format!("({:?} {})", op, operand.display_with_table(string_table))
            }

            HirExprKind::Call { target, args } => {
                let args_str = args
                    .iter()
                    .map(|arg| arg.display_with_table(string_table))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({})", string_table.resolve(*target), args_str)
            }

            HirExprKind::MethodCall {
                receiver,
                method,
                args,
            } => {
                let args_str = args
                    .iter()
                    .map(|arg| arg.display_with_table(string_table))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{}.{}({})",
                    receiver.display_with_table(string_table),
                    string_table.resolve(*method),
                    args_str
                )
            }

            HirExprKind::StructConstruct { type_name, fields } => {
                let mut result = format!("{} {{ ", string_table.resolve(*type_name));
                for (i, (field_name, expr)) in fields.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&format!(
                        "{}: {}",
                        string_table.resolve(*field_name),
                        expr.display_with_table(string_table)
                    ));
                }
                result.push_str(" }");
                result
            }

            HirExprKind::Collection(exprs) => {
                let exprs_str = exprs
                    .iter()
                    .map(|e| e.display_with_table(string_table))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{}}}", exprs_str)
            }

            HirExprKind::Range { start, end } => {
                format!(
                    "{}..{}",
                    start.display_with_table(string_table),
                    end.display_with_table(string_table)
                )
            }
        }
    }
}

impl Display for HirExprKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        // Note: This Display implementation shows StringID placeholders.
        // Use display_with_table() for debugging with resolved strings.
        match self {
            HirExprKind::Int(value) => write!(f, "{}", value),
            HirExprKind::Float(value) => write!(f, "{}", value),
            HirExprKind::Bool(value) => write!(f, "{}", value),
            HirExprKind::StringLiteral(value) => write!(f, "\"StringID({})\"", value.0),
            HirExprKind::HeapString(value) => write!(f, "heap_string(\"StringID({})\")", value.0),
            HirExprKind::Char(value) => write!(f, "'{}'", value),

            HirExprKind::Var(name) => write!(f, "StringID({})", name.0),

            HirExprKind::Field { base, field } => {
                write!(f, "StringID({}).StringID({})", base.0, field.0)
            }

            HirExprKind::Index { base, index } => {
                write!(f, "StringID({}).get({})", base.0, index)
            }

            HirExprKind::Move(name) => {
                write!(f, "move(StringID({}))", name.0)
            }

            HirExprKind::BinOp { left, op, right } => {
                write!(f, "({} {:?} {})", left, op, right)
            }

            HirExprKind::UnaryOp { op, operand } => {
                write!(f, "({:?} {})", op, operand)
            }

            HirExprKind::Call { target, args } => {
                write!(f, "StringID({})(", target.0)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }

            HirExprKind::MethodCall {
                receiver,
                method,
                args,
            } => {
                write!(f, "{}.StringID({})(", receiver, method.0)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }

            HirExprKind::StructConstruct { type_name, fields } => {
                write!(f, "StringID({}) {{ ", type_name.0)?;
                for (i, (field_name, expr)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "StringID({}): {}", field_name.0, expr)?;
                }
                write!(f, " }}")
            }

            HirExprKind::Collection(exprs) => {
                write!(f, "{{")?;
                for (i, expr) in exprs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", expr)?;
                }
                write!(f, "}}")
            }

            HirExprKind::Range { start, end } => {
                write!(f, "{}..{}", start, end)
            }
        }
    }
}

impl HirPattern {
    /// Display HIR pattern with resolved string IDs for debugging
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        match self {
            HirPattern::Literal(expr) => expr.display_with_table(string_table),
            HirPattern::Range { start, end } => {
                format!(
                    "{}..{}",
                    start.display_with_table(string_table),
                    end.display_with_table(string_table)
                )
            }
            HirPattern::Wildcard => "_".to_string(),
        }
    }
}

impl Display for HirPattern {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            HirPattern::Literal(expr) => write!(f, "{}", expr),
            HirPattern::Range { start, end } => write!(f, "{}..{}", start, end),
            HirPattern::Wildcard => write!(f, "_"),
        }
    }
}

impl Display for BinOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            BinOp::Add => write!(f, "+"),
            BinOp::Sub => write!(f, "-"),
            BinOp::Mul => write!(f, "*"),
            BinOp::Div => write!(f, "/"),
            BinOp::Mod => write!(f, "%"),
            BinOp::Eq => write!(f, "=="),
            BinOp::Ne => write!(f, "!="),
            BinOp::Lt => write!(f, "<"),
            BinOp::Le => write!(f, "<="),
            BinOp::Gt => write!(f, ">"),
            BinOp::Ge => write!(f, ">="),
            BinOp::And => write!(f, "&&"),
            BinOp::Or => write!(f, "||"),
            BinOp::Root => write!(f, "root"),
            BinOp::Exponent => write!(f, "^"),
        }
    }
}

impl Display for UnaryOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            UnaryOp::Neg => write!(f, "-"),
            UnaryOp::Not => write!(f, "!"),
        }
    }
}

impl HirBlock {
    /// Display HIR block with resolved string IDs for debugging
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        let mut result = format!("Block #{}:\n", self.id);
        for node in &self.nodes {
            result.push_str(&indent_lines(&node.display_with_table(string_table), 2));
        }
        result
    }
}

impl Display for HirBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        writeln!(f, "Block #{}:", self.id)?;
        for node in &self.nodes {
            write!(f, "{}", indent_lines(&node.to_string(), 2))?;
        }
        Ok(())
    }
}

impl HirModule {
    /// Display entire HIR module with resolved string IDs for debugging
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        let mut result = String::from("=== HIR Module ===\n\n");

        result.push_str("Entry Block: ");
        result.push_str(&self.entry_block.to_string());
        result.push_str("\n\n");

        result.push_str("Structs:\n");
        for struct_node in &self.structs {
            result.push_str(&struct_node.display_with_table(string_table));
            result.push_str("\n\n");
        }

        result.push_str("Functions:\n");
        for func_node in &self.functions {
            result.push_str(&func_node.display_with_table(string_table));
            result.push_str("\n\n");
        }

        result.push_str("Blocks:\n");
        for block in &self.blocks {
            result.push_str(&block.display_with_table(string_table));
            result.push_str("\n");
        }

        result
    }
}

impl Display for HirModule {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        writeln!(f, "=== HIR Module ===")?;
        writeln!(f)?;
        writeln!(f, "Entry Block: {}", self.entry_block)?;
        writeln!(f)?;

        writeln!(f, "Structs:")?;
        for struct_node in &self.structs {
            writeln!(f, "{}", struct_node)?;
            writeln!(f)?;
        }

        writeln!(f, "Functions:")?;
        for func_node in &self.functions {
            writeln!(f, "{}", func_node)?;
            writeln!(f)?;
        }

        writeln!(f, "Blocks:")?;
        for block in &self.blocks {
            writeln!(f, "{}", block)?;
        }

        Ok(())
    }
}
