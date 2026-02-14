//! Struct Layout and Type Conversion
//!
//! This module handles struct layout computation and type conversion
//! between Beanstalk DataTypes and LIR types.

use crate::compiler::datatypes::DataType;
use crate::compiler::hir::nodes::{HirExpr, HirExprKind};
use crate::compiler::lir::nodes::LirType;
use crate::compiler::parsers::ast_nodes::Var;
use crate::compiler::string_interning::InternedString;
use saying::say;

// ============================================================================
// Struct Layout
// ============================================================================

/// Describes the memory layout of a struct type.
#[derive(Debug, Clone)]
pub struct StructLayout {
    /// The name of the struct type
    pub name: InternedString,

    /// Layout information for each field, in declaration order
    pub fields: Vec<FieldLayout>,

    /// Total size of the struct in bytes, including any padding
    pub total_size: u32,
}

impl StructLayout {
    /// Creates a new StructLayout with the given name and no fields.
    pub fn new(name: InternedString) -> Self {
        Self {
            name,
            fields: Vec::new(),
            total_size: 0,
        }
    }

    /// Looks up a field by name and returns its layout information.
    pub fn get_field(&self, field_name: InternedString) -> Option<&FieldLayout> {
        self.fields.iter().find(|f| f.name == field_name)
    }
}

/// Describes the layout of a single struct field.
#[derive(Debug, Clone)]
pub struct FieldLayout {
    /// The name of the field
    pub name: InternedString,

    /// The byte offset of this field from the start of the struct
    pub offset: u32,

    /// The LIR type of this field
    pub ty: LirType,
}

impl FieldLayout {
    /// Creates a new FieldLayout with the given properties.
    pub fn new(name: InternedString, offset: u32, ty: LirType) -> Self {
        Self { name, offset, ty }
    }
}

// ============================================================================
// Type Size and Alignment
// ============================================================================

/// Computes the size in bytes for a given LIR type.
pub fn size_of_lir_type(ty: LirType) -> u32 {
    match ty {
        LirType::I32 => 4,
        LirType::I64 => 8,
        LirType::F32 => 4,
        LirType::F64 => 8,
    }
}

/// Computes the alignment requirement in bytes for a given LIR type.
pub fn alignment_of_lir_type(ty: LirType) -> u32 {
    size_of_lir_type(ty)
}

/// Maps an HIR expression to its corresponding LIR type based on its kind.
pub fn hir_expr_to_lir_type(expr: &HirExpr) -> LirType {
    match &expr.kind {
        HirExprKind::Int(_) => LirType::I64,
        HirExprKind::Float(_) => LirType::F64,
        HirExprKind::Bool(_) => LirType::I32,
        HirExprKind::Char(_) => LirType::I32,
        HirExprKind::HeapString(_) => LirType::I32,
        HirExprKind::StructConstruct { .. } => LirType::I32,
        HirExprKind::Collection(..) => LirType::I32,
        HirExprKind::Load(_) => LirType::I32,
        HirExprKind::Range { .. } => LirType::I32,
        HirExprKind::BinOp { left, .. } => hir_expr_to_lir_type(left),
        HirExprKind::UnaryOp { operand, .. } => hir_expr_to_lir_type(operand),
        _ => {
            say!(
                Red "Compiler Bug (possibly will lead to undefined behaviour): Unexpected HirExprKind in hir_expr_to_lir_type",
                #expr.kind
            );
            LirType::I32
        }
    }
}

/// Maps a Beanstalk DataType to its corresponding LIR type.
/// This is used for function signatures and other contexts where we still have DataType.
pub fn datatype_to_lir_type(data_type: &DataType) -> LirType {
    match data_type {
        DataType::Int => LirType::I64,
        DataType::Float => LirType::F64,
        DataType::Bool => LirType::I32,
        DataType::Char => LirType::I32,
        DataType::String => LirType::I32,
        DataType::Struct(_, _) => LirType::I32,
        DataType::Collection(_, _) => LirType::I32,
        DataType::Range => LirType::I32,
        DataType::Template => LirType::I32,
        DataType::Parameters(_) => LirType::I32,
        DataType::Returns(_) => LirType::I32,
        DataType::Function(_, _) => LirType::I32,
        DataType::Option(_) => LirType::I32,
        DataType::Choices(_) => LirType::I32,
        DataType::Reference(inner) => datatype_to_lir_type(inner),
        DataType::None => LirType::I32,
        DataType::Inferred => LirType::I32, // Should not appear after type checking
        DataType::True => LirType::I32,
        DataType::False => LirType::I32,
        DataType::CoerceToString => LirType::I32,
        DataType::Decimal => LirType::I32, // Decimals will be heap allocated
    }
}

// ============================================================================
// Struct Layout Computation
// ============================================================================

/// Aligns an offset to the specified alignment boundary.
fn align_to(offset: u32, alignment: u32) -> u32 {
    let mask = alignment - 1;
    (offset + mask) & !mask
}

/// Computes field offsets with proper alignment for a list of struct fields.
pub fn compute_field_offsets(fields: &[Var]) -> Vec<FieldLayout> {
    let mut field_layouts = Vec::with_capacity(fields.len());
    let mut current_offset: u32 = 0;

    for field in fields {
        let lir_type = datatype_to_lir_type(&field.value.data_type);
        let alignment = alignment_of_lir_type(lir_type);
        let size = size_of_lir_type(lir_type);

        let aligned_offset = align_to(current_offset, alignment);

        field_layouts.push(FieldLayout {
            name: field.id,
            offset: aligned_offset,
            ty: lir_type,
        });

        current_offset = aligned_offset + size;
    }

    field_layouts
}

/// Calculates the total size of a struct including any trailing padding.
pub fn calculate_struct_size(field_layouts: &[FieldLayout]) -> u32 {
    if field_layouts.is_empty() {
        return 0;
    }

    let max_alignment = field_layouts
        .iter()
        .map(|f| alignment_of_lir_type(f.ty))
        .max()
        .unwrap_or(1);

    let last_field = field_layouts.last().unwrap();
    let end_of_last_field = last_field.offset + size_of_lir_type(last_field.ty);

    align_to(end_of_last_field, max_alignment)
}

/// Builds a complete StructLayout from a HIR struct definition.
pub fn build_struct_layout(name: InternedString, fields: &[Var]) -> StructLayout {
    let field_layouts = compute_field_offsets(fields);
    let total_size = calculate_struct_size(&field_layouts);

    StructLayout {
        name,
        fields: field_layouts,
        total_size,
    }
}
