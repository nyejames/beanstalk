//! HIR Place model (scaffold)
//!
//! Represents memory locations for borrow checking and move analysis.

#[derive(Debug, Clone)]
pub enum PlaceKind {
    Local,
    Global,
}

#[derive(Debug, Clone)]
pub struct Place {
    pub kind: PlaceKind,
    pub name: String,
}
