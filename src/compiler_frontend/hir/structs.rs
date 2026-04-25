//! HIR struct declarations.
//!
//! WHAT: struct-level HIR metadata, including field types.
//! WHY: backends need struct layouts for construction, field access, and lowering.

use crate::compiler_frontend::hir::hir_datatypes::TypeId;
use crate::compiler_frontend::hir::ids::{FieldId, StructId};

#[derive(Debug, Clone)]
pub struct HirStruct {
    pub id: StructId,
    pub fields: Vec<HirField>,
}

#[derive(Debug, Clone)]
pub struct HirField {
    pub id: FieldId,
    pub ty: TypeId,
}
