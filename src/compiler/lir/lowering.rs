//! HIR → LIR lowering
//!
//! Transforms annotated HIR into LIR suitable for Wasm codegen.

use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::datatypes::DataType;
use crate::compiler::hir::nodes::{BinOp, HirExpr, HirExprKind, HirKind, HirNode};
use crate::compiler::hir::place::{Place, PlaceRoot};
use crate::compiler::lir::nodes::{LirField, LirFunction, LirInst, LirModule, LirStruct, LirType};
use crate::compiler::string_interning::{InternedString, StringTable};
use std::collections::HashMap;

/// Lower HIR into LIR.
pub fn lower_to_lir(
    hir: &[HirNode],
    string_table: &StringTable,
) -> Result<LirModule, CompilerError> {
    let mut lowerer = LirLowerer::new(string_table);
    lowerer.lower_module(hir)
}

struct LirLowerer<'a> {
    string_table: &'a StringTable,
    module: LirModule,
    local_map: HashMap<InternedString, u32>,
    next_local_index: u32,
}

impl<'a> LirLowerer<'a> {
    fn new(string_table: &'a StringTable) -> Self {
        Self {
            string_table,
            module: LirModule::default(),
            local_map: HashMap::new(),
            next_local_index: 0,
        }
    }

    fn lower_module(&mut self, hir: &[HirNode]) -> Result<LirModule, CompilerError> {
        // First pass: gather struct definitions and function signatures
        for node in hir {
            match &node.kind {
                HirKind::StructDef { name, fields } => {
                    let lir_struct = self.lower_struct(*name, fields)?;
                    self.module.structs.push(lir_struct);
                }
                _ => {}
            }
        }

        // Second pass: lower functions
        for node in hir {
            match &node.kind {
                HirKind::FunctionDef {
                    name,
                    signature,
                    body,
                } => {
                    let lir_func = self.lower_function(
                        name.resolve(self.string_table).to_string(),
                        signature,
                        body,
                    )?;
                    self.module.functions.push(lir_func);
                }
                _ => {}
            }
        }

        Ok(self.module.clone())
    }

    fn lower_struct(
        &self,
        name: InternedString,
        fields: &[crate::compiler::parsers::ast_nodes::Arg],
    ) -> Result<LirStruct, CompilerError> {
        let mut lir_fields = Vec::new();
        let mut current_offset = 0;

        for field in fields {
            let ty = lower_type(&field.value.data_type);
            let size = type_size(&ty);

            lir_fields.push(LirField {
                name: field.id,
                offset: current_offset,
                ty: ty.clone(),
            });

            current_offset += size;
        }

        Ok(LirStruct {
            name,
            fields: lir_fields,
            total_size: current_offset,
        })
    }

    fn lower_function(
        &mut self,
        name: String,
        signature: &crate::compiler::parsers::statements::functions::FunctionSignature,
        body: &[HirNode],
    ) -> Result<LirFunction, CompilerError> {
        self.local_map.clear();
        self.next_local_index = 0;

        let mut params = Vec::new();
        for param in &signature.parameters {
            let ty = lower_type(&param.value.data_type);
            params.push(ty);
            self.local_map.insert(param.id, self.next_local_index);
            self.next_local_index += 1;
        }

        let mut returns = Vec::new();
        for ret in &signature.returns {
            returns.push(lower_type(&ret.value.data_type));
        }

        let mut body_instructions = Vec::new();
        let mut function_locals = Vec::new();

        // Before lowering the body, we need to handle local variables.
        // In HIR, locals might be introduced via Assign nodes that don't exist yet in our map.

        for node in body {
            self.lower_node(node, &mut body_instructions, &mut function_locals)?;
        }

        Ok(LirFunction {
            name,
            params,
            returns,
            locals: function_locals,
            body: body_instructions,
            is_main: false, // TODO: detect main
        })
    }

    fn lower_node(
        &mut self,
        node: &HirNode,
        instructions: &mut Vec<LirInst>,
        function_locals: &mut Vec<LirType>,
    ) -> Result<(), CompilerError> {
        match &node.kind {
            HirKind::Assign { place, value } => {
                self.lower_expr(value, instructions, function_locals)?;
                let local_idx =
                    self.get_or_create_local(place, &value.data_type, function_locals)?;
                instructions.push(LirInst::LocalSet(local_idx));
            }
            HirKind::Return(places) => {
                for place in places {
                    let local_idx = self.get_local(place)?;
                    instructions.push(LirInst::LocalGet(local_idx));
                }
                instructions.push(LirInst::Return);
            }
            HirKind::ExprStmt(place) => {
                let local_idx = self.get_local(place)?;
                instructions.push(LirInst::LocalGet(local_idx));
                instructions.push(LirInst::Drop);
            }
            HirKind::Drop(place) => {
                // In Wasm, simple locals don't need explicit dropping from memory,
                // but if this was a heap-allocated type, we'd call a free function.
                // For now, we just emit a Nop or a placeholder if we want to track it.
                let _ = self.get_local(place)?;
                instructions.push(LirInst::Nop);
            }
            HirKind::If {
                condition,
                then_block,
                else_block,
            } => {
                let cond_idx = self.get_local(condition)?;
                instructions.push(LirInst::LocalGet(cond_idx));

                let mut then_inst = Vec::new();
                for n in then_block {
                    self.lower_node(n, &mut then_inst, function_locals)?;
                }

                let else_inst = if let Some(eb) = else_block {
                    let mut inst = Vec::new();
                    for n in eb {
                        self.lower_node(n, &mut inst, function_locals)?;
                    }
                    Some(inst)
                } else {
                    None
                };

                instructions.push(LirInst::If {
                    then_branch: then_inst,
                    else_branch: else_inst,
                });
            }
            HirKind::Loop { body, .. } => {
                let mut loop_body = Vec::new();
                for n in body {
                    self.lower_node(n, &mut loop_body, function_locals)?;
                }
                // In Wasm, a loop block allows branching to the start.
                // We'll need a way to actually exit the loop (BrIf/Br).
                instructions.push(LirInst::Loop {
                    instructions: loop_body,
                });
            }
            HirKind::Break { target } => {
                instructions.push(LirInst::Br(target.to_owned() as u32));
            }
            HirKind::Continue { target } => {
                instructions.push(LirInst::Br(target.to_owned() as u32));
            }
            HirKind::Call {
                target,
                args,
                returns,
            } => {
                for arg in args {
                    let idx = self.get_local(arg)?;
                    instructions.push(LirInst::LocalGet(idx));
                }

                // TODO: resolve target name to function index in the module
                // For now we use a placeholder index 0.
                instructions.push(LirInst::Call(0));

                // Handle returns by moving them into locals
                for ret in returns.iter().rev() {
                    let idx = self.get_or_create_local(ret, function_locals)?;
                    instructions.push(LirInst::LocalSet(idx));
                }
            }
            _ => {
                instructions.push(LirInst::Nop);
            }
        }
        Ok(())
    }

    fn lower_expr(
        &mut self,
        expr: &HirExpr,
        instructions: &mut Vec<LirInst>,
        function_locals: &mut Vec<LirType>,
    ) -> Result<(), CompilerError> {
        match &expr.kind {
            HirExprKind::Int(v) => {
                instructions.push(LirInst::I64Const(*v));
            }
            HirExprKind::Float(v) => {
                instructions.push(LirInst::F64Const(*v));
            }
            HirExprKind::Bool(v) => {
                instructions.push(LirInst::I32Const(if *v { 1 } else { 0 }));
            }
            HirExprKind::Load(place) => {
                let idx = self.get_local(place)?;
                instructions.push(LirInst::LocalGet(idx));
            }
            HirExprKind::BinOp { left, op, right } => {
                let left_idx = self.get_local(left)?;
                let right_idx = self.get_local(right)?;
                instructions.push(LirInst::LocalGet(left_idx));
                instructions.push(LirInst::LocalGet(right_idx));

                let lir_ty = lower_type(&expr.data_type);

                match (lir_ty, op) {
                    (LirType::I64, BinOp::Add) => instructions.push(LirInst::I64Add),
                    (LirType::I64, BinOp::Sub) => instructions.push(LirInst::I64Sub),
                    (LirType::I64, BinOp::Mul) => instructions.push(LirInst::I64Mul),
                    (LirType::I64, BinOp::Div) => instructions.push(LirInst::I64DivS),
                    (LirType::I64, BinOp::Eq) => instructions.push(LirInst::I64Eq),
                    (LirType::I64, BinOp::Ne) => instructions.push(LirInst::I64Ne),
                    (LirType::I64, BinOp::Lt) => instructions.push(LirInst::I64LtS),
                    (LirType::I64, BinOp::Gt) => instructions.push(LirInst::I64GtS),

                    (LirType::F64, BinOp::Add) => instructions.push(LirInst::F64Add),
                    (LirType::F64, BinOp::Sub) => instructions.push(LirInst::F64Sub),
                    (LirType::F64, BinOp::Mul) => instructions.push(LirInst::F64Mul),
                    (LirType::F64, BinOp::Div) => instructions.push(LirInst::F64Div),
                    (LirType::F64, BinOp::Eq) => instructions.push(LirInst::F64Eq),
                    (LirType::F64, BinOp::Ne) => instructions.push(LirInst::F64Ne),

                    _ => instructions.push(LirInst::Nop),
                }
            }
            _ => {
                instructions.push(LirInst::Nop);
            }
        }
        Ok(())
    }

    fn get_local(&self, place: &Place) -> Result<u32, CompilerError> {
        match &place.root {
            PlaceRoot::Local(name) | PlaceRoot::Param(name) => {
                self.local_map.get(name).cloned().ok_or_else(|| {
                    // This should really be a compiler error as HIR should be valid
                    CompilerError::compiler_error("Local not found in LIR lowering")
                })
            }
            _ => Err(CompilerError::compiler_error(
                "Unsupported place root in LIR",
            )),
        }
    }

    fn get_or_create_local(
        &mut self,
        place: &Place,
        data_type: &DataType,
        function_locals: &mut Vec<LirType>,
    ) -> Result<u32, CompilerError> {
        match &place.root {
            PlaceRoot::Local(name) => {
                if let Some(idx) = self.local_map.get(name) {
                    Ok(*idx)
                } else {
                    let idx = self.next_local_index;
                    self.local_map.insert(*name, idx);
                    self.next_local_index += 1;
                    function_locals.push(lower_type(data_type));
                    Ok(idx)
                }
            }
            PlaceRoot::Param(name) => self
                .local_map
                .get(name)
                .cloned()
                .ok_or_else(|| CompilerError::compiler_error("Param not found")),
            _ => Err(CompilerError::compiler_error("Unsupported place root")),
        }
    }
}

fn lower_type(ty: &DataType) -> LirType {
    match ty {
        DataType::Int => LirType::I64,
        DataType::Float => LirType::F64,
        DataType::Bool => LirType::I32,
        _ => LirType::I32, // Pointer
    }
}

fn type_size(ty: &LirType) -> u32 {
    match ty {
        LirType::I32 | LirType::F32 => 4,
        LirType::I64 | LirType::F64 => 8,
    }
}
