use crate::compiler::hir::nodes::{BinOp, HirExpr, HirExprKind, HirKind, HirNode, UnaryOp};
use crate::compiler::hir::place::Place;
use crate::compiler::string_interning::StringTable;
use std::fmt::{Display, Formatter, Result as FmtResult};

// === Display Implementations for HIR Debugging ===
impl HirNode {
    /// Display HIR node with resolved string IDs for debugging
    pub fn display_with_table(&self, string_table: &StringTable) -> String {
        let mut result = format!(
            "HIR Node #{} ({}:{}:{}): ",
            self.id,
            self.location.start_pos.line_number,
            self.location.start_pos.char_column,
            self.scope.to_string(string_table)
        );

        match &self.kind {
            HirKind::Assign { place, value } => {
                result.push_str("Assign\n");
                result.push_str(&format!(
                    "  Place: {}\n",
                    place.display_with_table(string_table)
                ));
                result.push_str(&format!(
                    "  Value: {}",
                    value.display_with_table(string_table)
                ));
            }

            HirKind::Borrow {
                place,
                kind,
                target,
            } => {
                result.push_str(&format!("Borrow ({:?})\n", kind));
                result.push_str(&format!(
                    "  Place: {}\n",
                    place.display_with_table(string_table)
                ));
                result.push_str(&format!(
                    "  Target: {}",
                    target.display_with_table(string_table)
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
                result.push_str("  Then:\n");
                for node in then_block {
                    result.push_str(&indent_lines(&node.display_with_table(string_table), 4));
                }
                if let Some(else_nodes) = else_block {
                    result.push_str("  Else:\n");
                    for node in else_nodes {
                        result.push_str(&indent_lines(&node.display_with_table(string_table), 4));
                    }
                }
            }

            HirKind::Match {
                scrutinee,
                arms,
                default,
            } => {
                result.push_str("Match\n");
                result.push_str(&format!(
                    "  Scrutinee: {}\n",
                    scrutinee.display_with_table(string_table)
                ));
                result.push_str("  Arms:\n");
                for (i, arm) in arms.iter().enumerate() {
                    result.push_str(&format!("    Arm {}:\n", i));
                    result.push_str(&format!("      Pattern: {:?}\n", arm.pattern));
                    if let Some(guard) = &arm.guard {
                        result.push_str(&format!(
                            "      Guard: {}\n",
                            guard.display_with_table(string_table)
                        ));
                    }
                    result.push_str("      Body:\n");
                    for node in &arm.body {
                        result.push_str(&indent_lines(&node.display_with_table(string_table), 8));
                    }
                }
                if let Some(default_nodes) = default {
                    result.push_str("  Default:\n");
                    for node in default_nodes {
                        result.push_str(&indent_lines(&node.display_with_table(string_table), 4));
                    }
                }
            }

            HirKind::Loop {
                label: id,
                binding,
                iterator,
                body,
                index_binding,
            } => {
                result.push_str("Loop\n");
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
                result.push_str(&format!(
                    "  Iterator: {}\n",
                    iterator.display_with_table(string_table)
                ));
                result.push_str("  Body:\n");
                for node in body {
                    result.push_str(&indent_lines(&node.display_with_table(string_table), 4));
                }
            }

            HirKind::Break { .. } => result.push_str("Break"),
            HirKind::Continue { .. } => result.push_str("Continue"),

            HirKind::Call {
                target,
                args,
                returns,
            } => {
                result.push_str("Call\n");
                result.push_str(&format!("  Target: {}\n", string_table.resolve(*target)));
                result.push_str(&format!(
                    "  Args: [{}]\n",
                    format_place_list_with_table(args, string_table)
                ));
                result.push_str(&format!(
                    "  Returns: [{}]",
                    format_place_list_with_table(returns, string_table)
                ));
            }

            HirKind::HostCall {
                target,
                module,
                import,
                args,
                returns,
            } => {
                result.push_str("HostCall\n");
                result.push_str(&format!("  Target: {}\n", string_table.resolve(*target)));
                result.push_str(&format!("  Module: {}\n", string_table.resolve(*module)));
                result.push_str(&format!("  Import: {}\n", string_table.resolve(*import)));
                result.push_str(&format!(
                    "  Args: [{}]\n",
                    format_place_list_with_table(args, string_table)
                ));
                result.push_str(&format!(
                    "  Returns: [{}]",
                    format_place_list_with_table(returns, string_table)
                ));
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
                result.push_str("  Error Handler:\n");
                for node in error_handler {
                    result.push_str(&indent_lines(&node.display_with_table(string_table), 4));
                }
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

            HirKind::Return(places) => {
                result.push_str(&format!(
                    "Return [{}]",
                    format_place_list_with_table(places, string_table)
                ));
            }

            HirKind::ReturnError(place) => {
                result.push_str(&format!(
                    "ReturnError {}",
                    place.display_with_table(string_table)
                ));
            }

            HirKind::Drop(place) => {
                result.push_str(&format!("Drop {}", place.display_with_table(string_table)));
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
                result.push_str("  Body:\n");
                for node in body {
                    result.push_str(&indent_lines(&node.display_with_table(string_table), 4));
                }
            }

            HirKind::FunctionDef {
                name,
                signature,
                body,
            } => {
                result.push_str(&format!("FunctionDef {}\n", string_table.resolve(*name)));
                result.push_str(&format!("  Signature: {:?}\n", signature));
                result.push_str("  Body:\n");
                for node in body {
                    result.push_str(&indent_lines(&node.display_with_table(string_table), 4));
                }
            }

            HirKind::StructDef { name, fields } => {
                result.push_str(&format!("StructDef {}\n", string_table.resolve(*name)));
                result.push_str("  Fields:\n");
                for field in fields {
                    result.push_str(&format!("    {:?}\n", field));
                }
            }

            HirKind::ExprStmt(place) => {
                result.push_str(&format!(
                    "ExprStmt {}",
                    place.display_with_table(string_table)
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
            "HIR Node #{} ({}:{}:{:?}): ",
            self.id,
            self.location.start_pos.line_number,
            self.location.start_pos.char_column,
            self.scope
        )?;

        // Note: This Display implementation shows StringID placeholders.
        // Use display_with_table() for debugging with resolved strings.
        match &self.kind {
            HirKind::Assign { place, value } => {
                writeln!(f, "Assign")?;
                writeln!(f, "  Place: {}", place)?;
                write!(f, "  Value: {}", value)?;
            }

            HirKind::Borrow {
                place,
                kind,
                target,
            } => {
                writeln!(f, "Borrow ({:?})", kind)?;
                writeln!(f, "  Place: {}", place)?;
                write!(f, "  Target: {}", target)?;
            }

            HirKind::If {
                condition,
                then_block,
                else_block,
            } => {
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

            HirKind::Match {
                scrutinee,
                arms,
                default,
            } => {
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

            HirKind::Loop {
                label: id,
                binding,
                iterator,
                body,
                index_binding,
            } => {
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

            HirKind::Break { .. } => write!(f, "Break")?,
            HirKind::Continue { .. } => write!(f, "Continue")?,

            HirKind::Call {
                target,
                args,
                returns,
            } => {
                writeln!(f, "Call")?;
                writeln!(f, "  Target: {}", target)?;
                writeln!(f, "  Args: [{}]", format_place_list(args))?;
                write!(f, "  Returns: [{}]", format_place_list(returns))?;
            }

            HirKind::HostCall {
                target,
                module,
                import,
                args,
                returns,
            } => {
                writeln!(f, "HostCall")?;
                writeln!(f, "  Target: {}", target)?;
                writeln!(f, "  Module: {}", module)?;
                writeln!(f, "  Import: {}", import)?;
                writeln!(f, "  Args: [{}]", format_place_list(args))?;
                write!(f, "  Returns: [{}]", format_place_list(returns))?;
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

            HirKind::Return(places) => {
                write!(f, "Return [{}]", format_place_list(places))?;
            }

            HirKind::ReturnError(place) => {
                write!(f, "ReturnError {}", place)?;
            }

            HirKind::Drop(place) => {
                write!(f, "Drop {}", place)?;
            }

            HirKind::RuntimeTemplateCall {
                template_fn,
                captures,
                id,
            } => {
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

            HirKind::FunctionDef {
                name,
                signature,
                body,
            } => {
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

            HirExprKind::Load(place) => format!("Load({})", place.display_with_table(string_table)),
            HirExprKind::SharedBorrow(place) => {
                format!("SharedBorrow({})", place.display_with_table(string_table))
            }
            HirExprKind::MutableBorrow(place) => {
                format!("MutableBorrow({})", place.display_with_table(string_table))
            }
            HirExprKind::CandidateMove(place, _) => {
                format!("CandidateMove({})", place.display_with_table(string_table))
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
                format!(
                    "{}({})",
                    string_table.resolve(*target),
                    format_place_list_with_table(args, string_table)
                )
            }

            HirExprKind::MethodCall {
                receiver,
                method,
                args,
            } => {
                format!(
                    "{}.{}({})",
                    receiver.display_with_table(string_table),
                    string_table.resolve(*method),
                    format_place_list_with_table(args, string_table)
                )
            }

            HirExprKind::StructConstruct { type_name, fields } => {
                let mut result = format!("{} {{ ", string_table.resolve(*type_name));
                for (i, (field_name, place)) in fields.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&format!(
                        "{}: {}",
                        string_table.resolve(*field_name),
                        place.display_with_table(string_table)
                    ));
                }
                result.push_str(" }");
                result
            }

            HirExprKind::Collection(places) => {
                format!("[{}]", format_place_list_with_table(places, string_table))
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
            HirExprKind::StringLiteral(value) => write!(f, "\"{}\"", value),
            HirExprKind::HeapString(value) => write!(f, "heap_string(\"{}\")", value),
            HirExprKind::Char(value) => write!(f, "'{}'", value),

            HirExprKind::Load(place) => write!(f, "Load({})", place),
            HirExprKind::SharedBorrow(place) => write!(f, "SharedBorrow({})", place),
            HirExprKind::MutableBorrow(place) => write!(f, "MutableBorrow({})", place),
            HirExprKind::CandidateMove(place, _) => write!(f, "CandidateMove({})", place),

            HirExprKind::BinOp { left, op, right } => {
                write!(f, "({} {} {})", left, op, right)
            }

            HirExprKind::UnaryOp { op, operand } => {
                write!(f, "({} {})", op, operand)
            }

            HirExprKind::Call { target, args } => {
                write!(f, "{}({})", target, format_place_list(args))
            }

            HirExprKind::MethodCall {
                receiver,
                method,
                args,
            } => {
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
    places
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_place_list_with_table(places: &[Place], string_table: &StringTable) -> String {
    places
        .iter()
        .map(|p| p.display_with_table(string_table))
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
