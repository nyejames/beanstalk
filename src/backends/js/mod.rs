//! JavaScript backend for Beanstalk.
//!
//! This backend lowers HIR into readable JavaScript using GC semantics.
//! Borrowing and ownership are optimization concerns and therefore ignored here.

mod js_expr;
mod js_function;
mod js_host_functions;
mod js_statement;

#[cfg(test)]
mod tests;

use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FieldId, FunctionId, HirBlock, HirModule, HirTerminator, LocalId,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::TextLocation;
use std::collections::{HashMap, HashSet, VecDeque};

/// Configuration for JS lowering.
#[derive(Debug, Clone)]
pub struct JsLoweringConfig {
    /// Emit human-readable formatting.
    pub pretty: bool,

    /// Emit source location comments.
    pub emit_locations: bool,

    /// Automatically invoke the module start function.
    pub auto_invoke_start: bool,
}

/// Result of lowering a HIR module to JavaScript.
#[derive(Debug, Clone)]
pub struct JsModule {
    /// Complete JS source code.
    pub source: String,
    pub function_name_by_id: HashMap<FunctionId, String>,
}

pub fn lower_hir_to_js(
    hir: &HirModule,
    string_table: &StringTable,
    config: JsLoweringConfig,
) -> Result<JsModule, CompilerError> {
    let mut emitter = JsEmitter::new(hir, string_table, config);
    emitter.lower_module()
}

pub(crate) struct JsEmitter<'hir> {
    pub(crate) hir: &'hir HirModule,
    pub(crate) string_table: &'hir StringTable,
    pub(crate) config: JsLoweringConfig,

    pub(crate) out: String,
    pub(crate) indent: usize,

    pub(crate) blocks_by_id: HashMap<BlockId, &'hir HirBlock>,

    pub(crate) function_name_by_id: HashMap<FunctionId, String>,
    pub(crate) function_name_by_path: HashMap<InternedPath, String>,
    pub(crate) local_name_by_id: HashMap<LocalId, String>,
    pub(crate) field_name_by_id: HashMap<FieldId, String>,

    used_identifiers: HashSet<String>,
    temp_counter: usize,
}

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn new(
        hir: &'hir HirModule,
        string_table: &'hir StringTable,
        config: JsLoweringConfig,
    ) -> Self {
        let blocks_by_id = hir
            .blocks
            .iter()
            .map(|block| (block.id, block))
            .collect::<HashMap<_, _>>();

        Self {
            hir,
            string_table,
            config,
            out: String::new(),
            indent: 0,
            blocks_by_id,
            function_name_by_id: HashMap::new(),
            function_name_by_path: HashMap::new(),
            local_name_by_id: HashMap::new(),
            field_name_by_id: HashMap::new(),
            used_identifiers: HashSet::new(),
            temp_counter: 0,
        }
    }

    fn lower_module(&mut self) -> Result<JsModule, CompilerError> {
        self.build_symbol_maps();

        let mut functions = self.hir.functions.iter().collect::<Vec<_>>();
        functions.sort_by_key(|function| function.id.0);

        for (index, function) in functions.into_iter().enumerate() {
            if index > 0 {
                self.emit_line("");
            }

            self.emit_function(function)?;
        }

        if self.config.auto_invoke_start {
            let Some(start_name) = self
                .function_name_by_id
                .get(&self.hir.start_function)
                .cloned()
            else {
                return Err(CompilerError::compiler_error(format!(
                    "JavaScript backend: start function {:?} has no generated JS name",
                    self.hir.start_function
                )));
            };

            if !self.out.is_empty() {
                self.emit_line("");
            }

            self.emit_line(&format!("{}();", start_name));
        }

        Ok(JsModule {
            source: self.out.clone(),
            function_name_by_id: self.function_name_by_id.clone(),
        })
    }

    fn build_symbol_maps(&mut self) {
        self.build_function_symbols();
        self.build_local_symbols();
        self.build_field_symbols();
    }

    fn build_function_symbols(&mut self) {
        let mut function_specs = self
            .hir
            .functions
            .iter()
            .map(|function| {
                let path = self.hir.side_table.function_name_path(function.id).cloned();
                let raw_name = path
                    .as_ref()
                    .map(|value| value.to_string(self.string_table))
                    .unwrap_or_else(|| format!("fn{}", function.id.0));

                (function.id, path, raw_name)
            })
            .collect::<Vec<_>>();

        function_specs.sort_by_key(|(id, _, _)| id.0);

        for (function_id, path, raw_name) in function_specs {
            let js_name = self.assign_unique_identifier(&raw_name);
            self.function_name_by_id
                .insert(function_id, js_name.clone());

            if let Some(path) = path {
                self.function_name_by_path.insert(path, js_name);
            }
        }
    }

    fn build_local_symbols(&mut self) {
        let mut local_specs = Vec::new();

        for block in &self.hir.blocks {
            for local in &block.locals {
                let raw_name = self
                    .hir
                    .side_table
                    .local_name_path(local.id)
                    .and_then(|path| path.name_str(self.string_table))
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("l{}", local.id.0));

                local_specs.push((local.id, raw_name));
            }
        }

        local_specs.sort_by_key(|(local_id, _)| local_id.0);
        local_specs.dedup_by_key(|(local_id, _)| local_id.0);

        for (local_id, raw_name) in local_specs {
            let js_name = self.assign_unique_identifier(&raw_name);
            self.local_name_by_id.insert(local_id, js_name);
        }
    }

    fn build_field_symbols(&mut self) {
        let mut field_specs = Vec::new();

        for hir_struct in &self.hir.structs {
            for field in &hir_struct.fields {
                let raw_name = self
                    .hir
                    .side_table
                    .field_name_path(field.id)
                    .and_then(|path| path.name_str(self.string_table))
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("field{}", field.id.0));

                field_specs.push((field.id, raw_name));
            }
        }

        field_specs.sort_by_key(|(field_id, _)| field_id.0);
        field_specs.dedup_by_key(|(field_id, _)| field_id.0);

        for (field_id, raw_name) in field_specs {
            let js_name = self.assign_unique_identifier(&raw_name);
            self.field_name_by_id.insert(field_id, js_name);
        }
    }

    fn assign_unique_identifier(&mut self, raw: &str) -> String {
        let mut identifier = sanitize_identifier(raw);

        if is_js_reserved(&identifier) {
            identifier = format!("_{}", identifier);
        }

        if identifier.is_empty() {
            identifier = "_value".to_owned();
        }

        let mut candidate = identifier.clone();
        let mut suffix = 1usize;

        while self.used_identifiers.contains(&candidate) {
            candidate = format!("{}_{}", identifier, suffix);
            suffix += 1;
        }

        self.used_identifiers.insert(candidate.clone());
        candidate
    }

    pub(crate) fn next_temp_identifier(&mut self, prefix: &str) -> String {
        loop {
            let raw = format!("{}_{}", prefix, self.temp_counter);
            self.temp_counter += 1;
            let candidate = sanitize_identifier(&raw);

            if !self.used_identifiers.contains(&candidate) {
                self.used_identifiers.insert(candidate.clone());
                return candidate;
            }
        }
    }

    pub(crate) fn function_name(&self, function_id: FunctionId) -> Result<&str, CompilerError> {
        self.function_name_by_id
            .get(&function_id)
            .map(String::as_str)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "JavaScript backend: missing function symbol for {:?}",
                    function_id
                ))
            })
    }

    pub(crate) fn user_call_name(&self, path: &InternedPath) -> Result<&str, CompilerError> {
        self.function_name_by_path
            .get(path)
            .map(String::as_str)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "JavaScript backend: unresolved user function call target '{}'",
                    path.to_string(self.string_table)
                ))
            })
    }

    pub(crate) fn local_name(&self, local_id: LocalId) -> Result<&str, CompilerError> {
        self.local_name_by_id
            .get(&local_id)
            .map(String::as_str)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "JavaScript backend: missing local symbol for {:?}",
                    local_id
                ))
            })
    }

    pub(crate) fn field_name(&self, field_id: FieldId) -> Result<&str, CompilerError> {
        self.field_name_by_id
            .get(&field_id)
            .map(String::as_str)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "JavaScript backend: missing field symbol for {:?}",
                    field_id
                ))
            })
    }

    pub(crate) fn block_by_id(&self, block_id: BlockId) -> Result<&'hir HirBlock, CompilerError> {
        self.blocks_by_id.get(&block_id).copied().ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "JavaScript backend: block {:?} not found in HIR module",
                block_id
            ))
        })
    }

    pub(crate) fn collect_reachable_blocks(
        &self,
        entry_block: BlockId,
    ) -> Result<Vec<BlockId>, CompilerError> {
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        let mut order = Vec::new();

        queue.push_back(entry_block);

        while let Some(block_id) = queue.pop_front() {
            if !visited.insert(block_id) {
                continue;
            }

            let block = self.block_by_id(block_id)?;
            order.push(block_id);

            for successor in Self::terminator_successors(&block.terminator) {
                queue.push_back(successor);
            }
        }

        order.sort_by_key(|block_id| block_id.0);
        Ok(order)
    }

    pub(crate) fn terminator_successors(terminator: &HirTerminator) -> Vec<BlockId> {
        match terminator {
            HirTerminator::Jump { target, .. } => vec![*target],
            HirTerminator::If {
                then_block,
                else_block,
                ..
            } => vec![*then_block, *else_block],
            HirTerminator::Match { arms, .. } => arms.iter().map(|arm| arm.body).collect(),
            HirTerminator::Loop { body, break_target } => vec![*body, *break_target],
            HirTerminator::Break { target } => vec![*target],
            HirTerminator::Continue { target } => vec![*target],
            HirTerminator::Return(_) | HirTerminator::Panic { .. } => vec![],
        }
    }

    pub(crate) fn emit_line(&mut self, line: &str) {
        if self.config.pretty {
            for _ in 0..self.indent {
                self.out.push_str("    ");
            }
        }

        self.out.push_str(line);
        self.out.push('\n');
    }

    pub(crate) fn emit_location_comment(&mut self, location: &TextLocation) {
        if !self.config.emit_locations {
            return;
        }

        let line = location.start_pos.line_number + 1;
        let start = location.start_pos.char_column;
        let end = location.end_pos.char_column;
        self.emit_line(&format!("// source {}:{}-{}", line, start, end));
    }

    pub(crate) fn with_indent<F>(&mut self, mut callback: F)
    where
        F: FnMut(&mut Self),
    {
        self.indent += 1;
        callback(self);
        self.indent -= 1;
    }
}

fn sanitize_identifier(raw: &str) -> String {
    let mut result = String::new();

    for (index, ch) in raw.chars().enumerate() {
        let is_valid = if index == 0 {
            ch == '_' || ch == '$' || ch.is_ascii_alphabetic()
        } else {
            ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
        };

        if is_valid {
            result.push(ch);
        } else {
            result.push('_');
        }
    }

    if result.is_empty() {
        "_value".to_owned()
    } else if result
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_digit())
    {
        format!("_{}", result)
    } else {
        result
    }
}

fn is_js_reserved(name: &str) -> bool {
    matches!(
        name,
        "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "export"
            | "extends"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "new"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
            | "enum"
            | "implements"
            | "interface"
            | "let"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "static"
            | "await"
            | "abstract"
            | "boolean"
            | "byte"
            | "char"
            | "double"
            | "final"
            | "float"
            | "goto"
            | "int"
            | "long"
            | "native"
            | "short"
            | "synchronized"
            | "throws"
            | "transient"
            | "volatile"
            | "undefined"
            | "null"
            | "true"
            | "false"
            | "NaN"
            | "Infinity"
            | "eval"
            | "arguments"
    )
}
