//! HIR core node definitions (scaffold)
//!
//! This module defines the minimal data structures for the High-Level IR (HIR)
//! that other stages (borrow checker, lowering) can reference. These are
//! intentionally lightweight placeholders and will evolve as the compiler is
//! implemented.

use std::path::PathBuf;

/// A complete HIR module for a single source file or compilation unit.
#[derive(Debug, Default, Clone)]
pub struct HirModule {
    pub source_path: Option<PathBuf>,
    pub functions: Vec<HirFunction>,
}

/// A HIR function with a structured body.
#[derive(Debug, Default, Clone)]
pub struct HirFunction {
    pub name: String,
    pub params: Vec<HirParam>,
    pub locals: Vec<HirLocal>,
    pub body: Vec<HirStmt>,
}

#[derive(Debug, Default, Clone)]
pub struct HirParam {
    pub name: String,
}

#[derive(Debug, Default, Clone)]
pub struct HirLocal {
    pub name: String,
}

/// Minimal set of HIR statements for scaffolding.
#[derive(Debug, Clone)]
pub enum HirStmt {
    Nop,
    Return,
}

impl Default for HirStmt {
    fn default() -> Self { HirStmt::Nop }
}
