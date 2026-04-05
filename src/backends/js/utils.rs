//! Output formatting, symbol lookups, and CFG traversal helpers for the JavaScript backend.
//!
//! Also provides the identifier sanitization utilities used by the symbol-map builder.

use crate::backends::js::JsEmitter;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FieldId, FunctionId, HirBlock, HirTerminator, LocalId,
};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::collections::{HashSet, VecDeque};

impl<'hir> JsEmitter<'hir> {
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
                    "JavaScript backend: missing function symbol for {function_id:?}",
                ))
            })
    }

    pub(crate) fn local_name(&self, local_id: LocalId) -> Result<&str, CompilerError> {
        self.local_name_by_id
            .get(&local_id)
            .map(String::as_str)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "JavaScript backend: missing local symbol for {local_id:?}"
                ))
            })
    }

    pub(crate) fn field_name(&self, field_id: FieldId) -> Result<&str, CompilerError> {
        self.field_name_by_id
            .get(&field_id)
            .map(String::as_str)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "JavaScript backend: missing field symbol for {field_id:?}"
                ))
            })
    }

    pub(crate) fn block_by_id(&self, block_id: BlockId) -> Result<&'hir HirBlock, CompilerError> {
        self.blocks_by_id.get(&block_id).copied().ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "JavaScript backend: block {block_id:?} not found in HIR module"
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
        self.emit_line(&format!("// source {line}:{start}-{end}"));
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

pub(crate) fn sanitize_identifier(raw: &str) -> String {
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
        format!("_{result}")
    } else {
        result
    }
}

pub(crate) fn is_js_reserved(name: &str) -> bool {
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
