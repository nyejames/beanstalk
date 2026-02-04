//! Display implementations for HIR nodes
//!
//! Provides human-readable string representations of HIR structures for debugging.
//! Types that require a StringTable use `display_with_table()` methods.
//! Types that don't need string resolution implement the standard `Display` trait.

use crate::compiler::hir::nodes::{
    BinOp, HirBlock, HirExpr, HirExprKind, HirKind, HirMatchArm, HirModule, HirNode, HirPattern,
    HirPlace, HirStmt, HirTerminator, UnaryOp,
};
use crate::compiler::string_interning::StringTable;
use std::fmt::{Display, Formatter, Result as FmtResult};

// === Display Implementations for types that don't need StringTable ===

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

// === Helper Functions ===

/// Indents each line of text by the specified number of spaces
fn indent_lines(text: &str, spaces: usize) -> String {
    let indent = " ".repeat(spaces);
    text.lines()
        .map(|line| format!("{}{}\n", indent, line))
        .collect()
}

// === Display with StringTable implementations ===

impl HirPlace {
    /// Displays the HIR place with resolved string IDs
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        match self {
            HirPlace::Var(name) => string_table.resolve(*name).to_string(),
            HirPlace::Field { base, field } => {
                format!(
                    "{}.{}",
                    base.display_with_table(string_table),
                    string_table.resolve(*field)
                )
            }
            HirPlace::Index { base, index } => {
                format!(
                    "{}[{}]",
                    base.display_with_table(string_table),
                    index.display_with_table(string_table)
                )
            }
        }
    }
}

impl HirExpr {
    /// Displays the HIR expression with resolved string IDs
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        format!(
            "{} : {:?}",
            self.kind.display_with_table(string_table),
            self.data_type
        )
    }
}

impl HirExprKind {
    /// Displays the HIR expression kind with resolved string IDs
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

            HirExprKind::Load(place) => place.display_with_table(string_table),

            HirExprKind::Field { base, field } => {
                format!(
                    "{}.{}",
                    string_table.resolve(*base),
                    string_table.resolve(*field)
                )
            }

            HirExprKind::Move(place) => {
                format!("move({})", place.display_with_table(string_table))
            }

            HirExprKind::BinOp { left, op, right } => {
                format!(
                    "({} {} {})",
                    left.display_with_table(string_table),
                    op,
                    right.display_with_table(string_table)
                )
            }

            HirExprKind::UnaryOp { op, operand } => {
                format!("({} {})", op, operand.display_with_table(string_table))
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

impl HirPattern {
    /// Displays the HIR pattern with resolved string IDs
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

impl HirMatchArm {
    /// Displays the HIR match arm with resolved string IDs
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        let mut result = format!("Pattern: {}", self.pattern.display_with_table(string_table));
        if let Some(guard) = &self.guard {
            result.push_str(&format!(" if {}", guard.display_with_table(string_table)));
        }
        result.push_str(&format!(" => Block #{}", self.body));
        result
    }
}

impl HirStmt {
    /// Displays the HIR statement with resolved string IDs
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        match self {
            HirStmt::Assign {
                target,
                value,
                is_mutable,
            } => {
                let mut_marker = if *is_mutable { " ~=" } else { " =" };
                format!(
                    "Assign: {}{} {}",
                    target.display_with_table(string_table),
                    mut_marker,
                    value.display_with_table(string_table)
                )
            }

            HirStmt::Call { target, args } => {
                let args_str = args
                    .iter()
                    .map(|arg| arg.display_with_table(string_table))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("Call: {}({})", string_table.resolve(*target), args_str)
            }

            HirStmt::HostCall { target, args } => {
                let args_str = args
                    .iter()
                    .map(|arg| arg.display_with_table(string_table))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("HostCall: {}({})", string_table.resolve(*target), args_str)
            }

            HirStmt::PossibleDrop(place) => {
                format!("PossibleDrop: {}", place.display_with_table(string_table))
            }

            HirStmt::RuntimeTemplateCall {
                template_fn,
                captures,
                id,
            } => {
                let mut result = format!(
                    "RuntimeTemplateCall: {}",
                    string_table.resolve(*template_fn)
                );
                if let Some(template_id) = id {
                    result.push_str(&format!(" @{}", string_table.resolve(*template_id)));
                }
                if !captures.is_empty() {
                    let captures_str = captures
                        .iter()
                        .map(|c| c.display_with_table(string_table))
                        .collect::<Vec<_>>()
                        .join(", ");
                    result.push_str(&format!(" captures: [{}]", captures_str));
                }
                result
            }

            HirStmt::TemplateFn { name, params, body } => {
                let params_str = params
                    .iter()
                    .map(|(n, t)| format!("{}: {:?}", string_table.resolve(*n), t))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "TemplateFn: {}({}) -> Block #{}",
                    string_table.resolve(*name),
                    params_str,
                    body
                )
            }

            HirStmt::FunctionDef {
                name,
                signature,
                body,
            } => {
                format!(
                    "FunctionDef: {} {:?} -> Block #{}",
                    string_table.resolve(*name),
                    signature,
                    body
                )
            }

            HirStmt::StructDef { name, fields } => {
                let fields_str = fields
                    .iter()
                    .map(|f| format!("{:?}", f))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "StructDef: {} {{ {} }}",
                    string_table.resolve(*name),
                    fields_str
                )
            }

            HirStmt::ExprStmt(expr) => {
                format!("ExprStmt: {}", expr.display_with_table(string_table))
            }
        }
    }
}

impl HirTerminator {
    /// Displays the HIR terminator with resolved string IDs
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        match self {
            HirTerminator::If {
                condition,
                then_block,
                else_block,
            } => {
                let mut result = format!(
                    "If: {} then Block #{}",
                    condition.display_with_table(string_table),
                    then_block
                );
                if let Some(else_id) = else_block {
                    result.push_str(&format!(" else Block #{}", else_id));
                }
                result
            }

            HirTerminator::Match {
                scrutinee,
                arms,
                default_block,
            } => {
                let mut result = format!("Match: {}\n", scrutinee.display_with_table(string_table));
                for (i, arm) in arms.iter().enumerate() {
                    result.push_str(&format!(
                        "    Arm {}: {}\n",
                        i,
                        arm.display_with_table(string_table)
                    ));
                }
                if let Some(default_id) = default_block {
                    result.push_str(&format!("    Default: Block #{}", default_id));
                }
                result
            }

            HirTerminator::Loop {
                label,
                binding,
                iterator,
                body,
                index_binding,
            } => {
                let mut result = format!("Loop @{}", label);
                if let Some((name, data_type)) = binding {
                    result.push_str(&format!(
                        " {} : {:?}",
                        string_table.resolve(*name),
                        data_type
                    ));
                }
                if let Some(index_name) = index_binding {
                    result.push_str(&format!(", index: {}", string_table.resolve(*index_name)));
                }
                if let Some(iter_expr) = iterator {
                    result.push_str(&format!(
                        " in {}",
                        iter_expr.display_with_table(string_table)
                    ));
                }
                result.push_str(&format!(" -> Block #{}", body));
                result
            }

            HirTerminator::Break { target } => {
                format!("Break -> Block #{}", target)
            }

            HirTerminator::Continue { target } => {
                format!("Continue -> Block #{}", target)
            }

            HirTerminator::Return(exprs) => {
                if exprs.is_empty() {
                    "Return".to_string()
                } else {
                    let exprs_str = exprs
                        .iter()
                        .map(|e| e.display_with_table(string_table))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("Return: {}", exprs_str)
                }
            }

            HirTerminator::ReturnError(expr) => {
                format!("ReturnError: {}", expr.display_with_table(string_table))
            }

            HirTerminator::Panic { message } => {
                if let Some(msg) = message {
                    format!("Panic: {}", msg.display_with_table(string_table))
                } else {
                    "Panic".to_string()
                }
            }
        }
    }
}

impl HirNode {
    /// Displays the HIR node with resolved string IDs
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        let location_str = format!(
            "{}:{}",
            self.location.start_pos.line_number, self.location.start_pos.char_column
        );

        let kind_str = match &self.kind {
            HirKind::Stmt(stmt) => stmt.display_with_table(string_table),
            HirKind::Terminator(term) => term.display_with_table(string_table),
        };

        format!("[#{}] ({}) {}", self.id, location_str, kind_str)
    }
}

impl HirBlock {
    /// Displays the HIR block with resolved string IDs
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        let mut result = format!("Block #{}:", self.id);

        if !self.params.is_empty() {
            let params_str = self
                .params
                .iter()
                .map(|p| string_table.resolve(*p).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            result.push_str(&format!(" (params: {})", params_str));
        }
        result.push('\n');

        for node in &self.nodes {
            result.push_str(&indent_lines(&node.display_with_table(string_table), 2));
        }

        result
    }
}

impl HirModule {
    /// Displays the entire HIR module with resolved string IDs
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        let mut result = String::from("=== HIR Module ===\n\n");

        result.push_str(&format!("Entry Block: #{}\n\n", self.entry_block));

        // Display struct definitions
        if !self.structs.is_empty() {
            result.push_str("--- Structs ---\n");
            for struct_node in &self.structs {
                result.push_str(&struct_node.display_with_table(string_table));
                result.push('\n');
            }
            result.push('\n');
        }

        // Display function definitions
        if !self.functions.is_empty() {
            result.push_str("--- Functions ---\n");
            for func_node in &self.functions {
                result.push_str(&func_node.display_with_table(string_table));
                result.push('\n');
            }
            result.push('\n');
        }

        // Display all blocks
        result.push_str("--- Blocks ---\n");
        for block in &self.blocks {
            result.push_str(&block.display_with_table(string_table));
            result.push('\n');
        }

        result
    }

    /// Creates a debug string representation of the HIR module
    /// This is a convenience method that creates a temporary string table display
    pub fn debug_string(&self, string_table: &StringTable) -> String {
        let mut result = String::new();

        result.push_str("╔══════════════════════════════════════════════════════════════╗\n");
        result.push_str("║                        HIR Module                            ║\n");
        result.push_str("╚══════════════════════════════════════════════════════════════╝\n\n");

        result.push_str(&format!("Entry Block: #{}\n", self.entry_block));
        result.push_str(&format!("Total Blocks: {}\n", self.blocks.len()));
        result.push_str(&format!("Functions: {}\n", self.functions.len()));
        result.push_str(&format!("Structs: {}\n\n", self.structs.len()));

        // Display struct definitions
        if !self.structs.is_empty() {
            result.push_str("┌─────────────────────────────────────────────────────────────┐\n");
            result.push_str("│ Struct Definitions                                          │\n");
            result.push_str("└─────────────────────────────────────────────────────────────┘\n");
            for struct_node in &self.structs {
                result.push_str(&struct_node.display_with_table(string_table));
                result.push('\n');
            }
            result.push('\n');
        }

        // Display function definitions
        if !self.functions.is_empty() {
            result.push_str("┌─────────────────────────────────────────────────────────────┐\n");
            result.push_str("│ Function Definitions                                        │\n");
            result.push_str("└─────────────────────────────────────────────────────────────┘\n");
            for func_node in &self.functions {
                result.push_str(&func_node.display_with_table(string_table));
                result.push('\n');
            }
            result.push('\n');
        }

        // Display blocks with their contents
        result.push_str("┌─────────────────────────────────────────────────────────────┐\n");
        result.push_str("│ Blocks                                                      │\n");
        result.push_str("└─────────────────────────────────────────────────────────────┘\n");
        for block in &self.blocks {
            result.push_str(&format!("\n▸ Block #{}", block.id));
            if !block.params.is_empty() {
                let params_str = block
                    .params
                    .iter()
                    .map(|p| string_table.resolve(*p).to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                result.push_str(&format!(" (params: {})", params_str));
            }
            result.push_str("\n");

            if block.nodes.is_empty() {
                result.push_str("  (empty)\n");
            } else {
                for node in &block.nodes {
                    result.push_str(&format!("  {}\n", node.display_with_table(string_table)));
                }
            }
        }

        result
    }
}
