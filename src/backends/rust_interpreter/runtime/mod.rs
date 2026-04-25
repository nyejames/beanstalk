//! Runtime execution scaffolding.
//!
//! WHAT: defines the core runtime containers used by the interpreter engine.
//! WHY: the runtime state should stay separate from lowering so CTFE can reuse the same engine later.

mod engine;
mod lookups;
mod operators;

pub(crate) use engine::RuntimeEngine;
