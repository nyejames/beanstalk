//! HIR Display
//!
//! Responsible for providing a way to get location and variable name information back from HIR.
//!
//! This will be used to help the rest of the HIR and borrow checker stages to create and return useful errors and warnings.
//! (CompilerMessages)
//! It will also enable printing out Hir structures for easy debugging also.

use crate::compiler_frontend::hir::hir_datatypes::{HirTypeKind, TypeContext, TypeId};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FieldId, FunctionId, HirBinOp, HirBlock, HirExpression, HirExpressionKind, HirField,
    HirFunction, HirLocal, HirMatchArm, HirModule, HirNodeId, HirPattern, HirPlace, HirStatement,
    HirStatementKind, HirStruct, HirTerminator, HirValueId, LocalId, OptionVariant, RegionId,
    ResultVariant, StructId, ValueKind,
};
use crate::compiler_frontend::hir::hir_side_table::HirSideTable;
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::string_interning::StringTable;
use std::fmt::{Display, Formatter, Result as FmtResult, Write as _};

const MAX_TYPE_RENDER_DEPTH: usize = 24;

#[derive(Debug, Clone, Copy)]
pub(crate) struct HirDisplayOptions {
    pub include_ids: bool,
    pub include_types: bool,
    pub include_value_kinds: bool,
    pub include_regions: bool,
    pub multiline_match_arms: bool,
}

impl Default for HirDisplayOptions {
    fn default() -> Self {
        Self {
            include_ids: true,
            include_types: true,
            include_value_kinds: false,
            include_regions: true,
            multiline_match_arms: true,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct HirDisplayContext<'a> {
    string_table: &'a StringTable,
    side_table: Option<&'a HirSideTable>,
    type_context: Option<&'a TypeContext>,
    options: HirDisplayOptions,
}

impl<'a> HirDisplayContext<'a> {
    pub(crate) fn new(string_table: &'a StringTable) -> Self {
        Self {
            string_table,
            side_table: None,
            type_context: None,
            options: HirDisplayOptions::default(),
        }
    }

    #[allow(dead_code)] // Used by feature-gated HIR logging and debug rendering helpers.
    pub(crate) fn with_side_table(mut self, side_table: &'a HirSideTable) -> Self {
        self.side_table = Some(side_table);
        self
    }

    #[allow(dead_code)] // Used by feature-gated HIR logging and debug rendering helpers.
    pub(crate) fn with_type_context(mut self, type_context: &'a TypeContext) -> Self {
        self.type_context = Some(type_context);
        self
    }

    pub(crate) fn render_module(&self, module: &HirModule) -> String {
        let mut out = String::with_capacity(
            module.blocks.len() * 160 + module.functions.len() * 64 + module.structs.len() * 64,
        );

        out.push_str("hir_module {\n");
        let _ = writeln!(
            out,
            "  start_function: {}",
            self.function_label(module.start_function)
        );
        let _ = writeln!(out, "  start_fragments: {}", module.start_fragments.len());
        let _ = writeln!(
            out,
            "  const_string_pool: {}",
            module.const_string_pool.len()
        );
        let _ = writeln!(out, "  doc_fragments: {}", module.doc_fragments.len());

        let _ = writeln!(out, "  regions: {}", module.regions.len());

        out.push_str("  functions:\n");
        if module.functions.is_empty() {
            out.push_str("    (none)\n");
        } else {
            for function in &module.functions {
                self.push_indented_line(&mut out, 4, &self.render_function(function));
            }
        }

        out.push_str("  structs:\n");
        if module.structs.is_empty() {
            out.push_str("    (none)\n");
        } else {
            for hir_struct in &module.structs {
                self.push_indented_line(&mut out, 4, &self.render_struct(hir_struct));
            }
        }

        out.push_str("  blocks:\n");
        if module.blocks.is_empty() {
            out.push_str("    (none)\n");
        } else {
            for block in &module.blocks {
                let block_rendered = self.render_block(block);
                self.push_indented_multiline(&mut out, 4, &block_rendered);
            }
        }

        if !module.warnings.is_empty() {
            let _ = writeln!(out, "  warnings: {}", module.warnings.len());
        }

        out.push('}');
        out
    }

    pub(crate) fn render_struct(&self, hir_struct: &HirStruct) -> String {
        let mut out = String::new();
        let _ = write!(out, "{} {{ ", self.struct_label(hir_struct.id));

        for (idx, field) in hir_struct.fields.iter().enumerate() {
            if idx > 0 {
                out.push_str(", ");
            }
            out.push_str(&self.render_field(field));
        }

        out.push_str(" }");
        out
    }

    pub(crate) fn render_field(&self, field: &HirField) -> String {
        if self.options.include_types {
            format!(
                "{}: {}",
                self.field_label(field.id),
                self.type_label(field.ty)
            )
        } else {
            self.field_label(field.id)
        }
    }

    pub(crate) fn render_function(&self, function: &HirFunction) -> String {
        let mut out = String::new();
        let params = function
            .params
            .iter()
            .map(|param| self.local_label(*param))
            .collect::<Vec<_>>()
            .join(", ");

        let _ = write!(out, "{}({})", self.function_label(function.id), params);

        if self.options.include_types {
            let _ = write!(out, " -> {}", self.type_label(function.return_type));
        }

        let _ = write!(out, " [entry: {}]", self.block_label(function.entry));
        out
    }

    pub(crate) fn render_block(&self, block: &HirBlock) -> String {
        let mut out = String::new();
        let _ = write!(out, "{} ", self.block_label(block.id));

        if self.options.include_regions {
            let _ = write!(out, "[region: {}]", self.region_label(block.region));
        }

        out.push('\n');

        if block.locals.is_empty() {
            out.push_str("  locals: (none)\n");
        } else {
            out.push_str("  locals:\n");
            for local in &block.locals {
                let rendered = self.render_local(local);
                self.push_indented_line(&mut out, 4, &rendered);
            }
        }

        if block.statements.is_empty() {
            out.push_str("  statements: (none)\n");
        } else {
            out.push_str("  statements:\n");
            for statement in &block.statements {
                let rendered = self.render_statement(statement);
                self.push_indented_line(&mut out, 4, &rendered);
            }
        }

        out.push_str("  terminator: ");
        out.push_str(&self.render_terminator(&block.terminator));
        out.push('\n');

        out
    }

    pub(crate) fn render_local(&self, local: &HirLocal) -> String {
        let mut out = String::new();

        if local.mutable {
            out.push_str("mut ");
        }

        out.push_str(&self.local_label(local.id));

        if self.options.include_types {
            let _ = write!(out, ": {}", self.type_label(local.ty));
        }

        if self.options.include_regions {
            let _ = write!(out, " [{}]", self.region_label(local.region));
        }

        out
    }

    pub(crate) fn render_statement(&self, statement: &HirStatement) -> String {
        let mut out = String::new();

        if self.options.include_ids {
            let _ = write!(out, "[{}] ", self.node_label(statement.id));
        }

        out.push_str(&self.render_statement_kind(&statement.kind));
        out
    }

    pub(crate) fn render_statement_kind(&self, kind: &HirStatementKind) -> String {
        match kind {
            HirStatementKind::Assign { target, value } => {
                format!(
                    "{} = {}",
                    self.render_place(target),
                    self.render_expression(value)
                )
            }
            HirStatementKind::Call {
                target,
                args,
                result,
            } => {
                let mut out = String::new();

                if let Some(local) = result {
                    let _ = write!(out, "{} = ", self.local_label(*local));
                }

                let args_rendered = args
                    .iter()
                    .map(|arg| self.render_expression(arg))
                    .collect::<Vec<_>>()
                    .join(", ");

                let _ = write!(
                    out,
                    "call {}({})",
                    self.render_call_target(target),
                    args_rendered
                );

                out
            }
            HirStatementKind::Expr(expr) => self.render_expression(expr),
            HirStatementKind::Drop(local) => format!("drop {}", self.local_label(*local)),
        }
    }

    pub(crate) fn render_terminator(&self, terminator: &HirTerminator) -> String {
        match terminator {
            HirTerminator::Jump { target, args } => {
                if args.is_empty() {
                    format!("jump {}", self.block_label(*target))
                } else {
                    let args_rendered = args
                        .iter()
                        .map(|arg| self.local_label(*arg))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("jump {}({})", self.block_label(*target), args_rendered)
                }
            }
            HirTerminator::If {
                condition,
                then_block,
                else_block,
            } => {
                format!(
                    "if {} -> {} else {}",
                    self.render_expression(condition),
                    self.block_label(*then_block),
                    self.block_label(*else_block)
                )
            }
            HirTerminator::Match { scrutinee, arms } => {
                if !self.options.multiline_match_arms {
                    let arms_rendered = arms
                        .iter()
                        .map(|arm| self.render_match_arm(arm))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return format!(
                        "match {} {{ {} }}",
                        self.render_expression(scrutinee),
                        arms_rendered
                    );
                }

                let mut out = String::new();
                let _ = writeln!(out, "match {} {{", self.render_expression(scrutinee));
                for arm in arms {
                    let _ = writeln!(out, "  {},", self.render_match_arm(arm));
                }
                out.push('}');
                out
            }
            HirTerminator::Loop { body, break_target } => {
                format!(
                    "loop body: {} break: {}",
                    self.block_label(*body),
                    self.block_label(*break_target)
                )
            }
            HirTerminator::Break { target } => format!("break {}", self.block_label(*target)),
            HirTerminator::Continue { target } => {
                format!("continue {}", self.block_label(*target))
            }
            HirTerminator::Return(value) => format!("return {}", self.render_expression(value)),
            HirTerminator::Panic { message } => match message {
                Some(msg) => format!("panic {}", self.render_expression(msg)),
                None => "panic".to_owned(),
            },
        }
    }

    pub(crate) fn render_expression(&self, expr: &HirExpression) -> String {
        let mut out = String::new();

        if self.options.include_ids {
            let _ = write!(out, "[{}] ", self.value_label(expr.id));
        }

        out.push_str(&self.render_expression_kind(&expr.kind));

        if self.options.include_types {
            let _ = write!(out, " : {}", self.type_label(expr.ty));
        }

        if self.options.include_value_kinds {
            let _ = write!(out, " [{}]", self.value_kind_label(expr.value_kind));
        }

        out
    }

    pub(crate) fn render_expression_kind(&self, kind: &HirExpressionKind) -> String {
        match kind {
            HirExpressionKind::Int(value) => value.to_string(),
            HirExpressionKind::Float(value) => value.to_string(),
            HirExpressionKind::Bool(value) => value.to_string(),
            HirExpressionKind::Char(value) => format!("'{}'", value.escape_debug()),
            HirExpressionKind::StringLiteral(value) => {
                format!("\"{}\"", value.escape_debug())
            }
            HirExpressionKind::Load(place) => self.render_place(place),
            HirExpressionKind::Copy(place) => format!("copy {}", self.render_place(place)),
            HirExpressionKind::BinOp { left, op, right } => format!(
                "({} {} {})",
                self.render_expression(left),
                op,
                self.render_expression(right)
            ),
            HirExpressionKind::UnaryOp { op, operand } => {
                format!("({}{})", op, self.render_expression(operand))
            }
            HirExpressionKind::StructConstruct { struct_id, fields } => {
                let mut out = String::new();
                let _ = write!(out, "{} {{ ", self.struct_label(*struct_id));

                for (idx, (field_id, expr)) in fields.iter().enumerate() {
                    if idx > 0 {
                        out.push_str(", ");
                    }
                    let _ = write!(
                        out,
                        "{}: {}",
                        self.field_label(*field_id),
                        self.render_expression(expr)
                    );
                }

                out.push_str(" }");
                out
            }
            HirExpressionKind::Collection(elements) => {
                let joined = elements
                    .iter()
                    .map(|element| self.render_expression(element))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{}]", joined)
            }
            HirExpressionKind::Range { start, end } => {
                format!(
                    "{}..{}",
                    self.render_expression(start),
                    self.render_expression(end)
                )
            }
            HirExpressionKind::TupleConstruct { elements } => {
                if elements.is_empty() {
                    return "()".to_owned();
                }

                let joined = elements
                    .iter()
                    .map(|element| self.render_expression(element))
                    .collect::<Vec<_>>()
                    .join(", ");

                if elements.len() == 1 {
                    format!("({},)", joined)
                } else {
                    format!("({})", joined)
                }
            }
            HirExpressionKind::OptionConstruct { variant, value } => match (variant, value) {
                (OptionVariant::Some, Some(expr)) => {
                    format!("Some({})", self.render_expression(expr))
                }
                (OptionVariant::None, None) => "None".to_owned(),
                (OptionVariant::Some, None) => "Some(<missing>)".to_owned(),
                (OptionVariant::None, Some(_)) => "None(<unexpected>)".to_owned(),
            },
            HirExpressionKind::ResultConstruct { variant, value } => {
                format!("{}({})", variant, self.render_expression(value))
            }
            HirExpressionKind::ResultPropagate { result } => {
                format!("propagate!({})", self.render_expression(result))
            }
            HirExpressionKind::ResultFallback { result, fallback } => format!(
                "fallback!({}, {})",
                self.render_expression(result),
                self.render_expression(fallback)
            ),
        }
    }

    pub(crate) fn render_place(&self, place: &HirPlace) -> String {
        match place {
            HirPlace::Local(local_id) => self.local_label(*local_id),
            HirPlace::Field { base, field } => {
                format!("{}.{}", self.render_place(base), self.field_label(*field))
            }
            HirPlace::Index { base, index } => {
                format!(
                    "{}[{}]",
                    self.render_place(base),
                    self.render_expression(index)
                )
            }
        }
    }

    pub(crate) fn render_pattern(&self, pattern: &HirPattern) -> String {
        match pattern {
            HirPattern::Literal(expr) => self.render_expression(expr),
            HirPattern::Wildcard => "_".to_owned(),
            HirPattern::Binding { local, subpattern } => match subpattern {
                Some(inner) => {
                    format!(
                        "{} @ {}",
                        self.local_label(*local),
                        self.render_pattern(inner)
                    )
                }
                None => self.local_label(*local),
            },
            HirPattern::Struct { struct_id, fields } => {
                let mut out = String::new();
                let _ = write!(out, "{} {{ ", self.struct_label(*struct_id));
                for (idx, (field_id, field_pattern)) in fields.iter().enumerate() {
                    if idx > 0 {
                        out.push_str(", ");
                    }
                    let _ = write!(
                        out,
                        "{}: {}",
                        self.field_label(*field_id),
                        self.render_pattern(field_pattern)
                    );
                }
                out.push_str(" }");
                out
            }
            HirPattern::Tuple { elements } => {
                let joined = elements
                    .iter()
                    .map(|element| self.render_pattern(element))
                    .collect::<Vec<_>>()
                    .join(", ");
                if elements.len() == 1 {
                    format!("({},)", joined)
                } else {
                    format!("({})", joined)
                }
            }
            HirPattern::Option {
                variant,
                inner_pattern,
            } => match (variant, inner_pattern) {
                (OptionVariant::Some, Some(pattern)) => {
                    format!("Some({})", self.render_pattern(pattern))
                }
                (OptionVariant::None, None) => "None".to_owned(),
                (OptionVariant::Some, None) => "Some(_)".to_owned(),
                (OptionVariant::None, Some(_)) => "None(_)".to_owned(),
            },
            HirPattern::Result {
                variant,
                inner_pattern,
            } => match inner_pattern {
                Some(pattern) => format!("{}({})", variant, self.render_pattern(pattern)),
                None => variant.to_string(),
            },
            HirPattern::Collection { elements, rest } => {
                let mut parts = elements
                    .iter()
                    .map(|element| self.render_pattern(element))
                    .collect::<Vec<_>>();

                if let Some(rest_local) = rest {
                    parts.push(format!("..{}", self.local_label(*rest_local)));
                }

                format!("[{}]", parts.join(", "))
            }
        }
    }

    pub(crate) fn render_match_arm(&self, arm: &HirMatchArm) -> String {
        let mut out = String::new();
        out.push_str(&self.render_pattern(&arm.pattern));

        if let Some(guard) = &arm.guard {
            let _ = write!(out, " if {}", self.render_expression(guard));
        }

        let _ = write!(out, " => {}", self.block_label(arm.body));
        out
    }

    fn render_call_target(&self, target: &CallTarget) -> String {
        match target {
            CallTarget::UserFunction(function_id) => self.function_label(*function_id),
            CallTarget::HostFunction(path) => path
                .name_str(self.string_table)
                .map(str::to_owned)
                .unwrap_or_else(|| path.to_string(self.string_table)),
        }
    }

    fn value_kind_label(&self, value_kind: ValueKind) -> &'static str {
        match value_kind {
            ValueKind::Place => "place",
            ValueKind::RValue => "rvalue",
            ValueKind::Const => "const",
        }
    }

    fn type_label(&self, ty: TypeId) -> String {
        let Some(type_context) = self.type_context else {
            return format!("t{}", ty.0);
        };

        self.render_type_with_context(type_context, ty, 0)
    }

    fn render_type_with_context(
        &self,
        type_context: &TypeContext,
        ty: TypeId,
        depth: usize,
    ) -> String {
        if depth >= MAX_TYPE_RENDER_DEPTH {
            return format!("t{}", ty.0);
        }

        let kind = &type_context.get(ty).kind;
        match kind {
            HirTypeKind::Bool => "Bool".to_owned(),
            HirTypeKind::Int => "Int".to_owned(),
            HirTypeKind::Float => "Float".to_owned(),
            HirTypeKind::Decimal => "Decimal".to_owned(),
            HirTypeKind::Char => "Char".to_owned(),
            HirTypeKind::String => "String".to_owned(),
            HirTypeKind::Range => "Range".to_owned(),
            HirTypeKind::Unit => "()".to_owned(),
            HirTypeKind::Tuple { fields } => {
                if fields.is_empty() {
                    return "()".to_owned();
                }

                let joined = fields
                    .iter()
                    .map(|field| self.render_type_with_context(type_context, *field, depth + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({})", joined)
            }
            HirTypeKind::Collection { element } => {
                format!(
                    "[{}]",
                    self.render_type_with_context(type_context, *element, depth + 1)
                )
            }
            HirTypeKind::Struct { struct_id } => self.struct_label(*struct_id),
            HirTypeKind::Function {
                receiver,
                params,
                returns,
            } => {
                let receiver = receiver.map(|recv| {
                    format!(
                        "{}.",
                        self.render_type_with_context(type_context, recv, depth + 1)
                    )
                });

                let params = params
                    .iter()
                    .map(|param| self.render_type_with_context(type_context, *param, depth + 1))
                    .collect::<Vec<_>>()
                    .join(", ");

                let returns = if returns.is_empty() {
                    "()".to_owned()
                } else if returns.len() == 1 {
                    self.render_type_with_context(type_context, returns[0], depth + 1)
                } else {
                    let joined = returns
                        .iter()
                        .map(|ret| self.render_type_with_context(type_context, *ret, depth + 1))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("({})", joined)
                };

                match receiver {
                    Some(receiver) => format!("fn({}{})->{}", receiver, params, returns),
                    None => format!("fn({})->{}", params, returns),
                }
            }
            HirTypeKind::Option { inner } => {
                format!(
                    "Option<{}>",
                    self.render_type_with_context(type_context, *inner, depth + 1)
                )
            }
            HirTypeKind::Result { ok, err } => format!(
                "Result<{}, {}>",
                self.render_type_with_context(type_context, *ok, depth + 1),
                self.render_type_with_context(type_context, *err, depth + 1)
            ),
            HirTypeKind::Union { variants } => {
                let joined = variants
                    .iter()
                    .map(|variant| self.render_type_with_context(type_context, *variant, depth + 1))
                    .collect::<Vec<_>>()
                    .join(" | ");
                format!("union({})", joined)
            }
        }
    }

    fn local_label(&self, local_id: LocalId) -> String {
        if let Some(name) = self
            .side_table
            .and_then(|side| side.resolve_local_name(local_id, self.string_table))
        {
            return name.to_owned();
        }

        format!("l{}", local_id.0)
    }

    fn function_label(&self, function_id: FunctionId) -> String {
        if let Some(name) = self
            .side_table
            .and_then(|side| side.resolve_function_name(function_id, self.string_table))
        {
            return name.to_owned();
        }

        format!("fn{}", function_id.0)
    }

    fn struct_label(&self, struct_id: StructId) -> String {
        if let Some(name) = self
            .side_table
            .and_then(|side| side.resolve_struct_name(struct_id, self.string_table))
        {
            return name.to_owned();
        }

        format!("struct{}", struct_id.0)
    }

    fn field_label(&self, field_id: FieldId) -> String {
        if let Some(name) = self
            .side_table
            .and_then(|side| side.resolve_field_name(field_id, self.string_table))
        {
            return name.to_owned();
        }

        format!("field{}", field_id.0)
    }

    fn block_label(&self, block_id: BlockId) -> String {
        format!("bb{}", block_id.0)
    }

    fn node_label(&self, node_id: HirNodeId) -> String {
        format!("n{}", node_id.0)
    }

    fn value_label(&self, value_id: HirValueId) -> String {
        format!("v{}", value_id.0)
    }

    fn region_label(&self, region_id: RegionId) -> String {
        format!("r{}", region_id.0)
    }

    fn push_indented_line(&self, out: &mut String, indent: usize, line: &str) {
        for _ in 0..indent {
            out.push(' ');
        }
        out.push_str(line);
        out.push('\n');
    }

    fn push_indented_multiline(&self, out: &mut String, indent: usize, text: &str) {
        for line in text.lines() {
            self.push_indented_line(out, indent, line);
        }
    }
}

// ============================================================================
// Convenience Display Hooks
// ============================================================================

#[allow(dead_code)] // Used by debug dumps and HIR-focused tests
impl HirModule {
    pub(crate) fn display_with_table(&self, string_table: &StringTable) -> String {
        HirDisplayContext::new(string_table).render_module(self)
    }

    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_module(self)
    }

    pub(crate) fn debug_string(&self, string_table: &StringTable) -> String {
        self.display_with_table(string_table)
    }
}

#[allow(dead_code)] // Reserved for block-focused debug dumps
impl HirBlock {
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_block(self)
    }
}

#[allow(dead_code)] // Reserved for function-focused debug dumps
impl HirFunction {
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_function(self)
    }
}

#[allow(dead_code)] // Reserved for struct-focused debug dumps
impl HirStruct {
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_struct(self)
    }
}

#[allow(dead_code)] // Reserved for statement-focused debug dumps
impl HirStatement {
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_statement(self)
    }
}

impl HirTerminator {
    #[allow(dead_code)] // Used for tests
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_terminator(self)
    }
}

impl HirExpression {
    #[allow(dead_code)] // Used by lowering diagnostics and HIR tests
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_expression(self)
    }
}

#[allow(dead_code)] // Reserved for place-focused debug dumps
impl HirPlace {
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_place(self)
    }
}

#[allow(dead_code)] // Reserved for pattern-focused debug dumps
impl HirPattern {
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_pattern(self)
    }
}

#[allow(dead_code)] // Reserved for match-arm debug dumps
impl HirMatchArm {
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_match_arm(self)
    }
}

// ============================================================================
// Simple Token Displays
// ============================================================================

impl Display for BlockId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "bb{}", self.0)
    }
}

impl Display for FunctionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "fn{}", self.0)
    }
}

impl Display for StructId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "struct{}", self.0)
    }
}

impl Display for FieldId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "field{}", self.0)
    }
}

impl Display for LocalId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "l{}", self.0)
    }
}

impl Display for RegionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "r{}", self.0)
    }
}

impl Display for HirNodeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "n{}", self.0)
    }
}

impl Display for HirValueId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "v{}", self.0)
    }
}

impl Display for HirBinOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            HirBinOp::Add => write!(f, "+"),
            HirBinOp::Sub => write!(f, "-"),
            HirBinOp::Mul => write!(f, "*"),
            HirBinOp::Div => write!(f, "/"),
            HirBinOp::Mod => write!(f, "%"),
            HirBinOp::Eq => write!(f, "=="),
            HirBinOp::Ne => write!(f, "!="),
            HirBinOp::Lt => write!(f, "<"),
            HirBinOp::Le => write!(f, "<="),
            HirBinOp::Gt => write!(f, ">"),
            HirBinOp::Ge => write!(f, ">="),
            HirBinOp::And => write!(f, "&&"),
            HirBinOp::Or => write!(f, "||"),
            HirBinOp::Root => write!(f, "root"),
            HirBinOp::Exponent => write!(f, "^"),
        }
    }
}

impl Display for crate::compiler_frontend::hir::hir_nodes::HirUnaryOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            crate::compiler_frontend::hir::hir_nodes::HirUnaryOp::Neg => write!(f, "-"),
            crate::compiler_frontend::hir::hir_nodes::HirUnaryOp::Not => write!(f, "!"),
        }
    }
}

impl Display for OptionVariant {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            OptionVariant::Some => write!(f, "Some"),
            OptionVariant::None => write!(f, "None"),
        }
    }
}

impl Display for ResultVariant {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            ResultVariant::Ok => write!(f, "Ok"),
            ResultVariant::Err => write!(f, "Err"),
        }
    }
}
