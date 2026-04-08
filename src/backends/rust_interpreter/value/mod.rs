//! Runtime value model.
//!
//! WHAT: represents interpreter-visible runtime values and heap references.
//! WHY: keeping values small and explicit means tracing, CTFE restrictions, and runtime checks are simpler.

use crate::backends::rust_interpreter::heap::HeapHandle;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    Char(char),
    Handle(HeapHandle),
}
