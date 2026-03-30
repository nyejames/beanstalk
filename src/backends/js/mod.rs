//! JavaScript backend for Beanstalk.
//!
//! This backend lowers HIR into readable JavaScript using GC semantics.
//! Borrowing and ownership are optimization concerns and therefore ignored here.

mod js_expr;
mod js_function;
mod js_host_functions;
mod js_statement;

#[cfg(test)]
pub(crate) mod test_symbol_helpers;
#[cfg(test)]
mod tests;

use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FieldId, FunctionId, HirBlock, HirModule, HirTerminator, LocalId,
};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
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

impl JsLoweringConfig {
    /// Standard HTML builder lowering config.
    ///
    /// WHY: both JS-only and Wasm builder paths use the same JS lowering settings. Centralising
    /// this avoids the settings drifting independently across call sites.
    pub fn standard_html(release_build: bool) -> Self {
        JsLoweringConfig {
            pretty: !release_build,
            emit_locations: false,
            auto_invoke_start: false,
        }
    }
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
    borrow_analysis: &BorrowCheckReport,
    string_table: &StringTable,
    config: JsLoweringConfig,
) -> Result<JsModule, CompilerError> {
    let mut emitter = JsEmitter::new(hir, borrow_analysis, string_table, config);
    emitter.lower_module()
}

pub(crate) struct JsEmitter<'hir> {
    pub(crate) hir: &'hir HirModule,
    pub(crate) borrow_analysis: &'hir BorrowCheckReport,
    pub(crate) string_table: &'hir StringTable,
    pub(crate) config: JsLoweringConfig,

    pub(crate) out: String,
    pub(crate) indent: usize,

    pub(crate) blocks_by_id: HashMap<BlockId, &'hir HirBlock>,

    pub(crate) function_name_by_id: HashMap<FunctionId, String>,
    pub(crate) local_name_by_id: HashMap<LocalId, String>,
    pub(crate) field_name_by_id: HashMap<FieldId, String>,

    used_identifiers: HashSet<String>,
    temp_counter: usize,
}

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn new(
        hir: &'hir HirModule,
        borrow_analysis: &'hir BorrowCheckReport,
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
            borrow_analysis,
            string_table,
            config,
            out: String::new(),
            indent: 0,
            blocks_by_id,
            function_name_by_id: HashMap::new(),
            local_name_by_id: HashMap::new(),
            field_name_by_id: HashMap::new(),
            used_identifiers: HashSet::new(),
            temp_counter: 0,
        }
    }

    fn lower_module(&mut self) -> Result<JsModule, CompilerError> {
        self.build_symbol_maps();
        self.emit_runtime_prelude();

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

    fn emit_runtime_prelude(&mut self) {
        // The JS backend preserves Beanstalk's aliasing semantics by modeling locals and computed
        // places as explicit reference records. The prelude is the concrete JS model for those
        // semantics — it is not incidental helper code.
        //
        // Helper groups and their responsibilities:
        //   binding helpers      — reference record construction, parameter normalisation, slot
        //                          read/write, and alias-chain resolution
        //   alias helpers        — binding-mode transitions for borrow and value assignment
        //   computed-place helpers — closures capturing base reference + key for field/index access
        //   clone helpers        — deep value copy for explicit `copy` semantics
        self.emit_runtime_binding_helpers();
        self.emit_runtime_alias_helpers();
        self.emit_runtime_computed_place_helpers();
        self.emit_runtime_clone_helpers();
    }

    /// Emits the core binding and slot read/write helpers.
    ///
    /// WHAT: `__bs_is_ref` identifies reference records; `__bs_binding` constructs slot bindings;
    /// `__bs_param_binding` normalises call arguments from plain JS values or alias refs;
    /// `__bs_resolve` walks alias chains; `__bs_read`/`__bs_write` perform guarded slot or
    /// computed-place reads and writes.
    /// WHY: every local and parameter in emitted JS flows through this layer so higher-level
    /// emission code can assume uniform binding semantics.
    fn emit_runtime_binding_helpers(&mut self) {
        self.emit_line("function __bs_is_ref(value) {");
        self.with_indent(|emitter| {
            emitter.emit_line(
                "return value !== null && typeof value === \"object\" && value.__bs_ref === true;",
            );
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_binding(value) {");
        self.with_indent(|emitter| {
            emitter.emit_line(
                "return { __bs_ref: true, __bs_kind: \"binding\", __bs_mode: \"slot\", __bs_slot: { value }, __bs_target: null };",
            );
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_param_binding(value) {");
        self.with_indent(|emitter| {
            // Calls from JS hosts can pass plain values; Beanstalk-to-Beanstalk calls pass
            // reference records. Normalise both so function bodies only deal with bindings.
            emitter.emit_line("if (!__bs_is_ref(value)) {");
            emitter.with_indent(|em| em.emit_line("return __bs_binding(value);"));
            emitter.emit_line("}");
            emitter.emit_line("if (value.__bs_kind === \"binding\") {");
            emitter.with_indent(|em| em.emit_line("return value;"));
            emitter.emit_line("}");
            // Computed-place ref: wrap in an alias binding so callers get a uniform handle.
            emitter.emit_line("const binding = __bs_binding(undefined);");
            emitter.emit_line("binding.__bs_mode = \"alias\";");
            emitter.emit_line("binding.__bs_target = value;");
            emitter.emit_line("return binding;");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_resolve(ref) {");
        self.with_indent(|emitter| {
            // Walk alias chains until a slot binding or computed-place ref is reached.
            emitter.emit_line(
                "while (ref.__bs_kind === \"binding\" && ref.__bs_mode === \"alias\") {",
            );
            emitter.with_indent(|em| em.emit_line("ref = ref.__bs_target;"));
            emitter.emit_line("}");
            emitter.emit_line("return ref;");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_read(ref) {");
        self.with_indent(|emitter| {
            emitter.emit_line("const resolved = __bs_resolve(ref);");
            emitter.emit_line(
                "return resolved.__bs_kind === \"binding\" ? resolved.__bs_slot.value : resolved.__bs_get();",
            );
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_write(ref, value) {");
        self.with_indent(|emitter| {
            emitter.emit_line("const resolved = __bs_resolve(ref);");
            emitter.emit_line("if (resolved.__bs_kind === \"binding\") {");
            emitter.with_indent(|em| em.emit_line("resolved.__bs_slot.value = value;"));
            emitter.emit_line("} else {");
            emitter.with_indent(|em| em.emit_line("resolved.__bs_set(value);"));
            emitter.emit_line("}");
            emitter.emit_line("return value;");
        });
        self.emit_line("}");
        self.emit_line("");
    }

    /// Emits binding-mode transition helpers for borrow and value assignment.
    ///
    /// WHAT: `__bs_assign_borrow` makes a fresh slot binding point at another reference (alias
    /// mode), or write-through if already an alias; `__bs_assign_value` collapses an alias and
    /// writes a plain value into the binding's slot.
    /// WHY: Beanstalk has distinct borrow-assign and value-assign semantics that must map to
    /// distinct JS operations — conflating them would silently break aliasing.
    fn emit_runtime_alias_helpers(&mut self) {
        self.emit_line("function __bs_assign_borrow(binding, ref) {");
        self.with_indent(|emitter| {
            // If the binding is already an alias, write through to the existing target rather
            // than rebinding — this preserves the observable aliasing contract.
            emitter.emit_line("if (binding.__bs_mode === \"alias\") {");
            emitter.with_indent(|em| em.emit_line("return __bs_write(binding, __bs_read(ref));"));
            emitter.emit_line("}");
            emitter.emit_line("binding.__bs_mode = \"alias\";");
            emitter.emit_line("binding.__bs_target = ref;");
            emitter.emit_line("return binding;");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_assign_value(binding, value) {");
        self.with_indent(|emitter| {
            // If the binding is an alias, write through so the aliased location gets the value.
            emitter.emit_line("if (binding.__bs_mode === \"alias\") {");
            emitter.with_indent(|em| em.emit_line("return __bs_write(binding, value);"));
            emitter.emit_line("}");
            // Slot mode: clear any stale alias target and write directly.
            emitter.emit_line("binding.__bs_mode = \"slot\";");
            emitter.emit_line("binding.__bs_target = null;");
            emitter.emit_line("binding.__bs_slot.value = value;");
            emitter.emit_line("return binding;");
        });
        self.emit_line("}");
        self.emit_line("");
    }

    /// Emits computed-place helpers for field and index access.
    ///
    /// WHAT: `__bs_field` and `__bs_index` each return a computed-place record capturing the base
    /// reference and key. The record implements `__bs_get`/`__bs_set` so it composes correctly
    /// with `__bs_read` and `__bs_write`.
    /// WHY: struct field and collection index mutations must route through the same reference
    /// layer as slot bindings — returning a composable computed ref achieves this uniformly.
    fn emit_runtime_computed_place_helpers(&mut self) {
        self.emit_line("function __bs_field(baseRef, field) {");
        self.with_indent(|emitter| {
            emitter.emit_line("return {");
            emitter.with_indent(|em| {
                em.emit_line("__bs_ref: true,");
                em.emit_line("__bs_kind: \"computed\",");
                em.emit_line("__bs_get() {");
                em.with_indent(|inner| inner.emit_line("return __bs_read(baseRef)[field];"));
                em.emit_line("},");
                em.emit_line("__bs_set(value) {");
                em.with_indent(|inner| inner.emit_line("__bs_read(baseRef)[field] = value;"));
                em.emit_line("}");
            });
            emitter.emit_line("};");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_index(baseRef, index) {");
        self.with_indent(|emitter| {
            emitter.emit_line("return {");
            emitter.with_indent(|em| {
                em.emit_line("__bs_ref: true,");
                em.emit_line("__bs_kind: \"computed\",");
                em.emit_line("__bs_get() {");
                em.with_indent(|inner| inner.emit_line("return __bs_read(baseRef)[index];"));
                em.emit_line("},");
                em.emit_line("__bs_set(value) {");
                em.with_indent(|inner| inner.emit_line("__bs_read(baseRef)[index] = value;"));
                em.emit_line("}");
            });
            emitter.emit_line("};");
        });
        self.emit_line("}");
        self.emit_line("");
    }

    /// Emits the deep-copy helper for explicit `copy` semantics.
    ///
    /// WHAT: `__bs_clone_value` recursively copies arrays element-by-element and plain objects
    /// key-by-key; primitives are returned as-is.
    /// WHY: Beanstalk `copy` must produce a value that does not alias the original — a shallow
    /// copy would silently break that contract for nested structures.
    fn emit_runtime_clone_helpers(&mut self) {
        self.emit_line("function __bs_clone_value(value) {");
        self.with_indent(|emitter| {
            emitter.emit_line("if (Array.isArray(value)) {");
            emitter.with_indent(|em| em.emit_line("return value.map(__bs_clone_value);"));
            emitter.emit_line("}");
            emitter.emit_line("if (value !== null && typeof value === \"object\") {");
            emitter.with_indent(|em| {
                em.emit_line("const result = {};");
                em.emit_line("for (const key of Object.keys(value)) {");
                em.with_indent(|inner| {
                    inner.emit_line("result[key] = __bs_clone_value(value[key]);");
                });
                em.emit_line("}");
                em.emit_line("return result;");
            });
            emitter.emit_line("}");
            emitter.emit_line("return value;");
        });
        self.emit_line("}");
        self.emit_line("");
    }

    fn build_symbol_maps(&mut self) {
        self.build_function_symbols();
        self.build_local_symbols();
        self.build_field_symbols();
    }

    fn build_function_symbols(&mut self) {
        let mut function_ids = self
            .hir
            .functions
            .iter()
            .map(|function| function.id)
            .collect::<Vec<_>>();
        function_ids.sort_by_key(|function_id| function_id.0);

        for function_id in function_ids {
            let leaf_name_hint = self
                .hir
                .side_table
                .resolve_function_name(function_id, self.string_table)
                .unwrap_or("fn");
            let raw_name = self.build_symbol_raw("fn", function_id.0, leaf_name_hint);
            let js_name = self.assign_unique_identifier(&raw_name);
            self.function_name_by_id.insert(function_id, js_name);
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
                    .map(|leaf_name| self.build_symbol_raw("l", local.id.0, leaf_name))
                    .unwrap_or_else(|| self.build_symbol_raw("l", local.id.0, "local"));

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
                    .map(|leaf_name| self.build_symbol_raw("fld", field.id.0, leaf_name))
                    .unwrap_or_else(|| self.build_symbol_raw("fld", field.id.0, "field"));

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

    pub(crate) fn emit_location_comment(&mut self, location: &SourceLocation) {
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

    fn build_symbol_raw(&self, kind_tag: &str, id: u32, leaf_name_hint: &str) -> String {
        if self.use_release_symbol_names() {
            format!("b_{kind_tag}{id}")
        } else {
            format!("bst_{leaf_name_hint}_{kind_tag}{id}")
        }
    }

    fn use_release_symbol_names(&self) -> bool {
        !self.config.pretty
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
