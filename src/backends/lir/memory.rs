//! Memory Operations
//!
//! This module handles lowering memory operations including:
//! - Variable loads
//! - Struct field access and assignment
//! - Collection element access and assignment

use crate::backends::lir::nodes::{LirInst, LirType};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{HirExpr, HirPlace};
use crate::compiler_frontend::string_interning::InternedString;

use super::context::LoweringContext;
use super::types::hir_expr_to_lir_type;

/// Collection header size in bytes: [length: i32, capacity: i32, element_size: i32]
const COLLECTION_HEADER_SIZE: u32 = 12;

/// Default element size for I64 elements (8 bytes)
const ELEMENT_SIZE_I64: u32 = 8;

/// Placeholder function index for bounds checking
const BOUNDS_CHECK_FUNC_INDEX: u32 = 0;

impl LoweringContext {
    // ========================================================================
    // Variable Load
    // ========================================================================

    /// Lowers a place load to LIR instructions.
    pub fn lower_place_load(&mut self, place: &HirPlace) -> Result<Vec<LirInst>, CompilerError> {
        match place {
            HirPlace::Var(name) => {
                let local_idx = self.var_to_local.get(name).ok_or_else(|| {
                    CompilerError::lir_transformation(format!("Undefined variable: {}", name))
                })?;
                Ok(vec![LirInst::LocalGet(*local_idx)])
            }
            HirPlace::Field { base, field } => self.lower_field_access_load(base, *field),
            HirPlace::Index { base, index } => self.lower_collection_element_load(base, index),
        }
    }

    /// Converts a HirPlace to a string for error messages.
    pub fn place_to_string(&self, place: &HirPlace) -> String {
        match place {
            HirPlace::Var(name) => format!("{}", name),
            HirPlace::Field { base, field } => {
                format!("{}.{}", self.place_to_string(base), field)
            }
            HirPlace::Index { base, .. } => {
                format!("{}[...]", self.place_to_string(base))
            }
        }
    }

    // ========================================================================
    // Field Access
    // ========================================================================

    /// Lowers a struct field access to LIR instructions.
    fn lower_field_access_load(
        &mut self,
        base: &HirPlace,
        field: InternedString,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Load the base pointer
        insts.extend(self.lower_place_load(base)?);

        // Mask out the ownership bit
        insts.push(LirInst::MaskPointer);

        // Get the struct type and look up the field layout
        let struct_type = self.get_place_struct_type(base)?;
        let field_layout = self.get_field_layout(struct_type, field).ok_or_else(|| {
            CompilerError::lir_transformation(format!(
                "Unknown field '{}' in struct '{}'",
                field, struct_type
            ))
        })?;

        let field_offset = field_layout.offset;
        let field_ty = field_layout.ty;

        // Emit the load instruction
        let load_inst = self.emit_load_instruction(field_ty, field_offset);
        insts.push(load_inst);

        Ok(insts)
    }

    /// Gets the struct type name for a place.
    fn get_place_struct_type(&self, place: &HirPlace) -> Result<InternedString, CompilerError> {
        match place {
            HirPlace::Var(name) => self.get_variable_struct_type(*name),
            HirPlace::Field { base, field } => {
                let base_struct_type = self.get_place_struct_type(base)?;
                let field_layout =
                    self.get_field_layout(base_struct_type, *field)
                        .ok_or_else(|| {
                            CompilerError::lir_transformation(format!(
                                "Unknown field '{}' in struct '{}'",
                                field, base_struct_type
                            ))
                        })?;

                if field_layout.ty == LirType::I32 {
                    Err(CompilerError::lir_transformation(format!(
                        "Nested struct field access type resolution not yet implemented for field '{}'",
                        field
                    )))
                } else {
                    Err(CompilerError::lir_transformation(format!(
                        "Field '{}' is not a struct type",
                        field
                    )))
                }
            }
            HirPlace::Index { .. } => Err(CompilerError::lir_transformation(
                "Cannot determine struct type for indexed place",
            )),
        }
    }

    /// Gets the struct type for a variable by name.
    fn get_variable_struct_type(
        &self,
        _var_name: InternedString,
    ) -> Result<InternedString, CompilerError> {
        // Return the first struct layout as a fallback
        // A proper implementation would track variable -> type mappings
        if let Some(struct_name) = self.struct_layouts.keys().next() {
            Ok(*struct_name)
        } else {
            Err(CompilerError::lir_transformation(
                "No struct layouts registered - cannot determine variable struct type",
            ))
        }
    }

    /// Emits the appropriate load instruction for a given LIR type and offset.
    pub fn emit_load_instruction(&self, ty: LirType, offset: u32) -> LirInst {
        match ty {
            LirType::I32 => LirInst::I32Load { offset, align: 4 },
            LirType::I64 => LirInst::I64Load { offset, align: 8 },
            LirType::F32 => LirInst::F32Load { offset, align: 4 },
            LirType::F64 => LirInst::F64Load { offset, align: 8 },
        }
    }

    /// Emits the appropriate store instruction for a given LIR type and offset.
    pub fn emit_store_instruction(&self, ty: LirType, offset: u32) -> LirInst {
        match ty {
            LirType::I32 => LirInst::I32Store { offset, align: 4 },
            LirType::I64 => LirInst::I64Store { offset, align: 8 },
            LirType::F32 => LirInst::F32Store { offset, align: 4 },
            LirType::F64 => LirInst::F64Store { offset, align: 8 },
        }
    }

    // ========================================================================
    // Field Assignment
    // ========================================================================

    /// Lowers a struct field assignment to LIR instructions.
    pub fn lower_field_assignment(
        &mut self,
        base: &HirPlace,
        field: InternedString,
        value: &HirExpr,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Load the base pointer
        insts.extend(self.lower_place_load(base)?);

        // Mask out the ownership bit
        insts.push(LirInst::MaskPointer);

        // Get the struct type and look up the field layout
        let struct_type = self.get_place_struct_type(base)?;
        let field_layout = self.get_field_layout(struct_type, field).ok_or_else(|| {
            CompilerError::lir_transformation(format!(
                "Unknown field '{}' in struct '{}'",
                field, struct_type
            ))
        })?;

        let field_offset = field_layout.offset;
        let field_ty = field_layout.ty;

        // Lower the value expression
        insts.extend(self.lower_expr(value)?);

        // Emit the store instruction
        let store_inst = self.emit_store_instruction(field_ty, field_offset);
        insts.push(store_inst);

        Ok(insts)
    }

    // ========================================================================
    // Collection Element Access
    // ========================================================================

    /// Lowers a collection element access to LIR instructions.
    pub fn lower_collection_element_load(
        &mut self,
        base: &HirPlace,
        index: &HirExpr,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Load the base collection pointer
        insts.extend(self.lower_place_load(base)?);

        // Mask out the ownership bit
        insts.push(LirInst::MaskPointer);

        // Store the base pointer in a temporary local
        let base_ptr_local = self.local_allocator.allocate(LirType::I32);
        insts.push(LirInst::LocalTee(base_ptr_local));

        // Lower the index expression
        insts.extend(self.lower_expr(index)?);

        // Store the index in a temporary local
        let index_local = self.local_allocator.allocate(LirType::I64);
        insts.push(LirInst::LocalSet(index_local));

        // Emit bounds check
        insts.push(LirInst::LocalGet(base_ptr_local));
        insts.push(LirInst::LocalGet(index_local));
        insts.push(LirInst::Call(BOUNDS_CHECK_FUNC_INDEX));

        // Calculate element offset and emit load
        insts.push(LirInst::LocalGet(base_ptr_local));
        insts.push(LirInst::LocalGet(index_local));
        insts.push(LirInst::I64Const(ELEMENT_SIZE_I64 as i64));
        insts.push(LirInst::I64Mul);
        insts.push(LirInst::I64Const(COLLECTION_HEADER_SIZE as i64));
        insts.push(LirInst::I64Add);
        insts.push(LirInst::I32Const(0)); // Placeholder for i64 to i32 conversion
        insts.push(LirInst::I32Add);
        insts.push(LirInst::I64Load {
            offset: 0,
            align: 8,
        });

        // Free temporary locals
        self.local_allocator.free(base_ptr_local);
        self.local_allocator.free(index_local);

        Ok(insts)
    }

    // ========================================================================
    // Collection Element Assignment
    // ========================================================================

    /// Lowers a collection element assignment to LIR instructions.
    pub fn lower_collection_element_assignment(
        &mut self,
        base: &HirPlace,
        index: &HirExpr,
        value: &HirExpr,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Load the base collection pointer
        insts.extend(self.lower_place_load(base)?);

        // Mask out the ownership bit
        insts.push(LirInst::MaskPointer);

        // Store the base pointer in a temporary local
        let base_ptr_local = self.local_allocator.allocate(LirType::I32);
        insts.push(LirInst::LocalTee(base_ptr_local));

        // Lower the index expression
        insts.extend(self.lower_expr(index)?);

        // Store the index in a temporary local
        let index_local = self.local_allocator.allocate(LirType::I64);
        insts.push(LirInst::LocalSet(index_local));

        // Emit bounds check
        insts.push(LirInst::LocalGet(base_ptr_local));
        insts.push(LirInst::LocalGet(index_local));
        insts.push(LirInst::Call(BOUNDS_CHECK_FUNC_INDEX));

        // Calculate element address
        insts.push(LirInst::LocalGet(base_ptr_local));
        insts.push(LirInst::LocalGet(index_local));
        insts.push(LirInst::I64Const(ELEMENT_SIZE_I64 as i64));
        insts.push(LirInst::I64Mul);
        insts.push(LirInst::I64Const(COLLECTION_HEADER_SIZE as i64));
        insts.push(LirInst::I64Add);
        insts.push(LirInst::I32Const(0)); // Placeholder for i64 to i32 conversion
        insts.push(LirInst::I32Add);

        // Store the calculated address in a temporary
        let addr_local = self.local_allocator.allocate(LirType::I32);
        insts.push(LirInst::LocalSet(addr_local));

        // Lower the value expression
        insts.extend(self.lower_expr(value)?);

        // Swap order for WASM store (expects [addr, value])
        let value_local = self.local_allocator.allocate(LirType::I64);
        insts.push(LirInst::LocalSet(value_local));
        insts.push(LirInst::LocalGet(addr_local));
        insts.push(LirInst::LocalGet(value_local));

        // Emit store instruction
        insts.push(LirInst::I64Store {
            offset: 0,
            align: 8,
        });

        // Free temporary locals
        self.local_allocator.free(base_ptr_local);
        self.local_allocator.free(index_local);
        self.local_allocator.free(addr_local);
        self.local_allocator.free(value_local);

        Ok(insts)
    }

    // ========================================================================
    // Local Allocation Helpers
    // ========================================================================

    /// Gets or allocates a local for a place.
    pub fn get_or_allocate_local(
        &mut self,
        place: &HirPlace,
        value_expr: &HirExpr,
    ) -> Result<u32, CompilerError> {
        match place {
            HirPlace::Var(name) => {
                if let Some(&local_idx) = self.var_to_local.get(name) {
                    Ok(local_idx)
                } else {
                    let lir_type = hir_expr_to_lir_type(value_expr);
                    let local_idx = self.local_allocator.allocate(lir_type);
                    self.var_to_local.insert(*name, local_idx);
                    Ok(local_idx)
                }
            }
            HirPlace::Field { .. } | HirPlace::Index { .. } => Err(
                CompilerError::lir_transformation("Cannot allocate local for field or index place"),
            ),
        }
    }

    /// Gets the local index for a HirPlace.
    pub fn get_local_for_place(&self, place: &HirPlace) -> Result<u32, CompilerError> {
        match place {
            HirPlace::Var(name) => self.var_to_local.get(name).copied().ok_or_else(|| {
                CompilerError::lir_transformation(format!(
                    "Cannot get local for undefined variable: {}",
                    name
                ))
            }),
            HirPlace::Field { base, field } => Err(CompilerError::lir_transformation(format!(
                "Cannot get local for field access: {}.{} - use base pointer instead",
                self.place_to_string(base),
                field
            ))),
            HirPlace::Index { base, .. } => Err(CompilerError::lir_transformation(format!(
                "Cannot get local for index access: {}[...] - use base pointer instead",
                self.place_to_string(base)
            ))),
        }
    }
}
