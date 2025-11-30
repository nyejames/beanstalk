//! LIR node definitions (scaffold)
//!
//! Defines the Low-Level IR structures used by the Wasm-adjacent lowering stage.

/// A complete LIR module containing lowered functions.
#[derive(Debug, Default, Clone)]
pub struct LirModule {
    pub functions: Vec<LirFunction>,
}

#[derive(Debug, Default, Clone)]
pub struct LirFunction {
    pub name: String,
    pub body: Vec<LirInst>,
}

/// Minimal instruction set placeholder to allow scaffolding to compile.
#[derive(Debug, Clone)]
pub enum LirInst {
    Nop,
    Return,
}

impl Default for LirInst {
    fn default() -> Self { LirInst::Nop }
}
