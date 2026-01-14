use crate::compiler::hir::nodes::{
    BinOp, HirBlock, HirExprKind, HirKind, HirModule, HirNode, HirPattern, UnaryOp,
};
use crate::compiler::string_interning::{InternedString, StringTable};
use std::fmt::{Display, Formatter, Result as FmtResult};

// === Display Implementations for HIR module and nodes ===

impl HirModule {
    pub fn debug_string(&self, string_table: &StringTable) -> String {
        // TODO
    }
}

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

            HirExprKind::Field { base, field } => {
                format!(
                    "{}.{}",
                    string_table.resolve(*base),
                    string_table.resolve(*field)
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
