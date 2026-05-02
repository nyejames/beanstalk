# Option/Result Type Path Audit (Phase 0)

This audit inventories current Option/Result parsing, typing, and lowering paths so later generic/result hardening work can target real duplication and gaps.

## Inventory

| Path | Current location | Category | Action |
|---|---|---|---|
| Option parsing (`T?`) | `src/compiler_frontend/declaration_syntax/type_syntax.rs` | Correct shared infrastructure | Keep |
| `none` expression parsing | `src/compiler_frontend/ast/expressions/parse_expression_literals.rs` | Correct shared infrastructure | Keep |
| Option compatibility/coercion | `src/compiler_frontend/type_coercion/compatibility.rs` | Correct shared infrastructure | Keep |
| Result return-slot parsing (`Type!`) | `src/compiler_frontend/ast/statements/functions.rs` | Correct shared infrastructure | Keep |
| `return!` parsing | `src/compiler_frontend/ast/statements/result_handling/` | Correct shared infrastructure | Keep |
| `call(...)!` propagation parsing | `src/compiler_frontend/ast/statements/result_handling/propagation.rs` | Correct shared infrastructure | Keep |
| Fallback syntax (`expr ! fallback`) | `src/compiler_frontend/ast/statements/result_handling/` | Correct shared infrastructure | Keep |
| Named handler syntax (`err! ...:`) | `src/compiler_frontend/ast/statements/result_handling/` | Correct shared infrastructure | Keep |
| Result HIR expression lowering | `src/compiler_frontend/hir/` expression/statement lowering paths | Correct shared infrastructure | Keep |
| JS backend result lowering | `src/projects/html_project_backend/` JS lowering paths | Backend-only implementation detail | Keep, but preserve typed frontend/HIR contract |
| Collection `get()` result typing | collection builtin parsing + call validation | Correct shared infrastructure | Keep |
| Builtin `Error` optional fields | `src/compiler_frontend/builtins/error_type.rs` | Correct shared infrastructure | Keep |

## Phase 0 notes

- No Option/Result semantic changes are introduced in Phase 0.
- Generic substrate additions are structural and do not merge Option/Result semantics into ordinary user generics.
- Any future deduplication should preserve explicit Option/Result language behavior while sharing only infrastructure where semantics are truly equivalent.
