//! HIR struct layout metadata.
//!
//! WHAT: lowering-local struct layout with stable field IDs.
//! WHY: backends need struct layouts for construction, field access, and lowering. Semantic type
//!      identity lives in the frontend `TypeEnvironment`; this table provides only the stable
//!      `StructId`/`FieldId` indexes and field ordering used during HIR lowering and backend
//!      emission.

use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::hir::ids::{FieldId, StructId};

/// Lowering-local struct layout entry.
///
/// WHY: HIR expressions and backends reference fields by stable `FieldId`. The semantic definition
///      (field types, nominal identity) lives in `TypeEnvironment`; `frontend_type_id` traces this
///      entry back to the canonical type.
#[derive(Debug, Clone)]
pub struct HirStruct {
    pub id: StructId,

    /// Trace to the canonical frontend `TypeId` in `TypeEnvironment`.
    /// WHY: this field makes the lowering-local → semantic type link explicit.
    ///      Not all current consumers read it, but it is part of the HIR layout contract.
    pub frontend_type_id: TypeId,

    pub fields: Vec<HirField>,
}

#[derive(Debug, Clone)]
pub struct HirField {
    pub id: FieldId,
    pub ty: TypeId,
}
