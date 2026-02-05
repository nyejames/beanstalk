//! Statement and Definition Lowering
//!
//! This module handles lowering HIR statements, terminators, and definitions
//! (functions, structs) to LIR.

use crate::compiler::compiler_messages::compiler_errors::CompilerError;
use crate::compiler::datatypes::DataType;
use crate::compiler::hir::nodes::{BlockId, HirBlock, HirKind, HirNode, HirStmt, HirTerminator};
use crate::compiler::host_functions::registry::CallTarget;
use crate::compiler::lir::nodes::{LirField, LirFunction, LirInst, LirStruct, LirType};
use crate::compiler::lir::types::datatype_to_lir_type;
use crate::compiler::parsers::ast_nodes::Var;
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::string_interning::InternedString;

use super::context::LoweringContext;

impl LoweringContext {
    // ========================================================================
    // Block Lowering
    // ========================================================================

    /// Lowers a HIR block to a sequence of LIR instructions.
    pub fn lower_block(
        &mut self,
        block_id: BlockId,
        blocks: &[HirBlock],
    ) -> Result<Vec<LirInst>, CompilerError> {
        let block = blocks.iter().find(|b| b.id == block_id).ok_or_else(|| {
            CompilerError::lir_transformation(format!("Block not found: {}", block_id))
        })?;

        let mut insts = Vec::new();

        for node in &block.nodes {
            let node_insts = self.lower_hir_node(node, blocks)?;
            insts.extend(node_insts);
        }

        Ok(insts)
    }

    /// Lowers a single HIR node to LIR instructions.
    fn lower_hir_node(
        &mut self,
        node: &HirNode,
        blocks: &[HirBlock],
    ) -> Result<Vec<LirInst>, CompilerError> {
        match &node.kind {
            HirKind::Stmt(stmt) => self.lower_stmt(stmt, blocks),
            HirKind::Terminator(term) => self.lower_terminator(term, blocks),
        }
    }

    /// Lowers a HIR statement to LIR instructions.
    pub fn lower_stmt(
        &mut self,
        stmt: &HirStmt,
        blocks: &[HirBlock],
    ) -> Result<Vec<LirInst>, CompilerError> {
        match stmt {
            HirStmt::Assign {
                target,
                value,
                is_mutable,
            } => self.lower_assign(target, value, *is_mutable),

            HirStmt::Call { target, args } => match target {
                CallTarget::UserFunction(name) => self.lower_function_call(*name, args),
                CallTarget::HostFunction(id) => self.lower_host_call(*id, args),
            },

            HirStmt::PossibleDrop(place) => self.lower_possible_drop(place),

            HirStmt::ExprStmt(expr) => {
                let mut insts = self.lower_expr(expr)?;
                // Drop the result - all expressions produce a value
                insts.push(LirInst::Drop);
                Ok(insts)
            }

            HirStmt::FunctionDef { .. } => {
                // Function definitions are handled at the module level
                Ok(vec![])
            }

            HirStmt::StructDef { .. } => {
                // Struct definitions are handled at the module level
                Ok(vec![])
            }

            HirStmt::RuntimeTemplateCall {
                template_fn,
                captures,
                ..
            } => self.lower_function_call(*template_fn, captures),

            HirStmt::TemplateFn { body, .. } => self.lower_block(*body, blocks),
        }
    }

    /// Lowers a HIR terminator to LIR instructions.
    pub fn lower_terminator(
        &mut self,
        term: &HirTerminator,
        blocks: &[HirBlock],
    ) -> Result<Vec<LirInst>, CompilerError> {
        match term {
            HirTerminator::If {
                condition,
                then_block,
                else_block,
            } => self.lower_if(condition, *then_block, *else_block, blocks),

            HirTerminator::Match {
                scrutinee,
                arms,
                default_block,
            } => self.lower_match(scrutinee, arms, *default_block, blocks),

            HirTerminator::Loop {
                label,
                binding,
                iterator,
                body,
                index_binding,
            } => self.lower_loop(
                *label,
                binding.clone(),
                iterator.as_ref(),
                *body,
                *index_binding,
                blocks,
            ),

            HirTerminator::Break { target } => self.lower_break(*target),

            HirTerminator::Continue { target } => self.lower_continue(*target),

            HirTerminator::Return(values) => self.lower_return(values),

            HirTerminator::ReturnError(error) => self.lower_return_error(error),

            HirTerminator::Panic { message } => self.lower_panic(message.as_ref()),
        }
    }

    // ========================================================================
    // Function Definition Lowering
    // ========================================================================

    /// Lowers a HIR function definition to a LirFunction.
    pub fn lower_function_def(
        &mut self,
        name: InternedString,
        signature: &FunctionSignature,
        body: BlockId,
        blocks: &[HirBlock],
        is_main: bool,
    ) -> Result<LirFunction, CompilerError> {
        // Reset context for the new function
        self.reset_for_function(name);

        // Extract parameter names and types from the signature
        let params: Vec<(InternedString, DataType)> = signature
            .parameters
            .iter()
            .map(|arg| (arg.id, arg.value.data_type.clone()))
            .collect();

        // Lower function parameters
        let prologue_insts = self.lower_function_parameters(&params)?;

        // Convert parameters to LIR types
        let param_types = self.params_to_lir_types(&params);

        // Lower the function body block
        let mut body_insts = prologue_insts;
        body_insts.extend(self.lower_block(body, blocks)?);

        // Collect local types from the allocator
        let all_local_types = self.local_allocator.get_local_types().to_vec();
        let num_params = param_types.len();
        let locals: Vec<LirType> = if all_local_types.len() > num_params {
            all_local_types[num_params..].to_vec()
        } else {
            Vec::new()
        };

        // Build return types from the signature
        let return_types: Vec<LirType> = signature
            .returns
            .iter()
            .map(|data_type| datatype_to_lir_type(data_type))
            .collect();

        // Build the complete LirFunction structure
        Ok(LirFunction {
            name: name.to_string(),
            params: param_types,
            returns: return_types,
            locals,
            body: body_insts,
            is_main,
        })
    }

    // ========================================================================
    // Struct Definition Lowering
    // ========================================================================

    /// Lowers a HIR struct definition to a LirStruct.
    pub fn lower_struct_def(
        &mut self,
        name: InternedString,
        fields: &[Var],
    ) -> Result<LirStruct, CompilerError> {
        // Register the struct layout
        self.register_struct_layout(name, fields);

        // Get the computed layout
        let layout = self.get_struct_layout(name).ok_or_else(|| {
            CompilerError::lir_transformation(format!(
                "Failed to compute struct layout for: {}",
                name
            ))
        })?;

        // Build the LirStruct structure
        let lir_fields: Vec<LirField> = layout
            .fields
            .iter()
            .map(|field| LirField {
                name: field.name,
                offset: field.offset,
                ty: field.ty,
            })
            .collect();

        Ok(LirStruct {
            name,
            fields: lir_fields,
            total_size: layout.total_size,
        })
    }
}
