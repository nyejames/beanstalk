//! Neutral top-level declaration shell parsers shared between the header stage and AST.
//!
//! WHAT: houses the parsers that produce top-level declaration shells from token streams.
//! These shells are produced in the header stage and consumed by AST — neither stage should
//! need to re-derive the syntax of the other's shells.
//!
//! WHY: keeping shell parsers in a module that both `headers` and `ast` can depend on removes
//! the need for direct cross-stage imports. The header stage calls these parsers to populate
//! header payloads; the AST stage re-uses them only for body-context expressions that share
//! the same syntax (e.g. inline struct literals).
//!
//! ## Stage contract
//!
//! - `headers` → `declaration_syntax` for shell parsers
//! - `ast`     → `declaration_syntax` for shell parsers
//! - `declaration_syntax` → `ast` for `ScopeContext` (read-only, no cycle at the item level)
//! - `declaration_syntax` must NOT import from `headers`

pub(crate) mod choice_shell;
pub(crate) mod signature_members;
pub(crate) mod struct_shell;
