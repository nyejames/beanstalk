//! Compiler-owned fixed symbol preseed infrastructure.
//!
//! WHAT: defines the set of compiler-owned symbols that are interned deterministically into every
//!      StringTable before per-file frontend preparation begins.
//! WHY: parallel tokenization and header parsing need stable IDs for fixed language/compiler names
//!      without sharing a mutable global table. Preseeding gives each local table the same symbol
//!      prefix with identical IDs.

use crate::compiler_frontend::builtins::error_type::{
    ERROR_FIELD_CODE, ERROR_FIELD_MESSAGE, ERROR_TYPE_NAME,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;

/// Typed accessors for every compiler-owned symbol ID.
///
/// WHAT: after preseeding a StringTable, callers receive this struct so they can refer to fixed
///      symbols by field name instead of by raw StringId.
/// WHY: field names prevent accidental mix-ups between symbol identities at call sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompilerSymbolIds {
    pub start: StringId,
    pub this: StringId,
    pub error: StringId,
    pub error_message: StringId,
    pub error_code: StringId,
    pub semicolon: StringId,
    pub closing_bracket: StringId,
    pub unknown_placeholder: StringId,
}

/// A StringTable that has been preseeded with the fixed compiler-owned symbol prefix.
///
/// WHAT: bundles the newly created table together with the stable IDs for every compiler symbol.
/// WHY: callers that create a fresh table for frontend work can receive both the table and the IDs
///      in one step, avoiding the error-prone pattern of creating a table and then preseeding it
///      separately.
///
/// # Stability invariant
/// Stable numeric IDs for compiler-owned symbols are only guaranteed when the table is preseeded
/// before any source or project strings are interned. Interning user strings before preseeding
/// will shift the compiler symbol IDs and break cross-table identity.
#[derive(Debug)]
pub struct PreseededStringTable {
    pub string_table: StringTable,
    pub compiler_symbol_ids: CompilerSymbolIds,
}

/// Owner for fixed compiler-owned symbols and deterministic table preseeding.
///
/// WHAT: interns the fixed compiler symbol set into a StringTable in a stable order.
/// WHY: every per-file local table starts from the same prefix so common compiler symbols share
///      the same IDs without cross-file coordination.
pub struct CompilerSymbolSet;

impl CompilerSymbolSet {
    /// Deterministically intern all fixed compiler symbols into `string_table`.
    ///
    /// Symbols are interned in declaration order so IDs are stable across independently created
    /// tables that start empty.
    ///
    /// # Stability invariant
    /// Stable numeric IDs for compiler-owned symbols require the table to be preseeded before any
    /// source or project strings are interned. If user strings are already present in the table,
    /// the returned IDs will not match the canonical compiler symbol IDs.
    pub fn preseed(string_table: &mut StringTable) -> CompilerSymbolIds {
        let start = string_table.intern(IMPLICIT_START_FUNC_NAME);
        let this = string_table.intern("this");
        let error = string_table.intern(ERROR_TYPE_NAME);
        let error_message = string_table.intern(ERROR_FIELD_MESSAGE);
        let error_code = string_table.intern(ERROR_FIELD_CODE);
        let semicolon = string_table.intern(";");
        let closing_bracket = string_table.intern("]");
        let unknown_placeholder = string_table.intern("<unknown>");

        CompilerSymbolIds {
            start,
            this,
            error,
            error_message,
            error_code,
            semicolon,
            closing_bracket,
            unknown_placeholder,
        }
    }

    /// Create a new `StringTable` with the given capacity and preseed it with compiler-owned symbols.
    ///
    /// This is the production entry point for frontend table construction. It guarantees that the
    /// table starts with the stable compiler symbol prefix before any source or project strings are
    /// interned.
    ///
    /// # Stability invariant
    /// Stable numeric IDs for compiler-owned symbols require the table to be preseeded before any
    /// source or project strings are interned. Callers must not intern strings into the returned
    /// table before using the preseeded IDs.
    pub fn preseeded_table(capacity: usize) -> PreseededStringTable {
        let mut string_table = StringTable::with_capacity(capacity);
        let compiler_symbol_ids = Self::preseed(&mut string_table);

        PreseededStringTable {
            string_table,
            compiler_symbol_ids,
        }
    }
}
