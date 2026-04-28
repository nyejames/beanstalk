//! HIR compile-time constants and documentation fragments.
//!
//! WHAT: data carried from AST into HIR for module constants and extracted documentation output.
//! WHY: constants are backend/tooling metadata, not ordinary runtime statements.

use crate::compiler_frontend::hir::hir_datatypes::TypeId;
use crate::compiler_frontend::hir::ids::HirConstId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HirDocFragmentKind {
    Doc,
}

#[derive(Debug, Clone)]
pub struct HirDocFragment {
    pub kind: HirDocFragmentKind,
    #[allow(dead_code)] // Used only in tests
    pub rendered_text: String,
    pub location: SourceLocation,
}

#[derive(Debug, Clone)]
pub struct HirConstField {
    pub name: String,
    pub value: HirConstValue,
}

#[derive(Debug, Clone)]
pub enum HirConstValue {
    #[allow(dead_code)]
    Int(i64),
    #[allow(dead_code)]
    Float(f64),
    #[allow(dead_code)]
    Bool(bool),
    #[allow(dead_code)]
    Char(char),
    String(String),
    Collection(Vec<HirConstValue>),
    Record(Vec<HirConstField>),
    Range(Box<HirConstValue>, Box<HirConstValue>),
    Result {
        #[allow(dead_code)]
        variant: crate::compiler_frontend::hir::expressions::ResultVariant,
        value: Box<HirConstValue>,
    },
    Choice {
        #[allow(dead_code)]
        tag: usize,
        fields: Vec<HirConstField>,
    },
}

#[derive(Debug, Clone)]
pub struct HirModuleConst {
    pub id: HirConstId,
    pub name: String,
    pub ty: TypeId,
    pub value: HirConstValue,
}
