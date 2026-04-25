# Beanstalk Roadmap
This is the main todo list for the language and compiler.

The current major goal is getting to a healthy alpha stage.
Each plan or PR that is needed will be linked here.

Use the language surface integration matrix as a reference for what is currently implemented: `docs/src/docs/progress/#page.bst`

---

# Next Plans

## Phase 1: File splitting tidy up **COMPLETE**
- Split parse_file_headers.rs.
- Split hir_nodes.rs.
- Add top-level module docs to datatypes.rs.
- Rename ResolvedConstFragment.html to content or rendered_text.

## Phase 2 — Declaration pipeline cleanup **COMPLETE**
Full Plan: `docs/roadmap/plans/dependency-sorting-cleanup.md`
- Remove declaration_stubs_by_path.
- Make dependency sorting produce the only sorted declaration list.
- Remove AST fallback stub append.
- Keep ModuleSymbols as symbol/import/export/source metadata only.
- Re-run full integration suite.

## Phase 3 — Type/access separation **COMPLETE**
Full Plan: `docs/roadmap/plans/type-access-separation.md`
- Introduce BindingAccess or similar.
- Stop storing Ownership inside collection/struct DataType.
- Move mutable/access state to declarations, locals, call arguments, HIR locals, and borrow facts.
- Keep compatibility checks type-only.
- Add tests proving mutability/access does not affect semantic type identity.

## Phase 4 - gating deffered systems
(this will be skipped for now)

## Phase 5 — Test pruning and consolidation
Full Plan: `docs/roadmap/plans/test-consolidation.md`
Keep integration tests as the main correctness layer.
Remove shallow unit tests that duplicate stable integration cases.
Keep unit tests for parser edge cases, diagnostic precision, HIR invariants, borrow facts, and test harness behavior.

---

# Notes / TODOS

## Review built in "Error" type and reserved keywords
Should this be build-system provided (like IO) rather than a compiler built in? So Error is reserved in a similar way to io and IO, and must always be provided by the build system, but the specific shape beyond the core parameters must be defined by the build system.

---

# Deferred until after Alpha
These are intentionally not Alpha blockers unless they become necessary for one of the supported slices.

This is a collection of notes and findings for future roadmaps once the roadmap above is complete.

- builtin `Error` enrichment beyond what is already required for the current compiler/runtime surface
- full tagged unions
- full pattern-matching design
- full interfaces implementation
- richer numeric redesign work not required by Alpha
- Compile time arbitary precision aritmetic + Decimals Type support
- Core Math library
- Optimised template folding
- Move to more specific explicit type declarations for numbers (I32, I64, F32, F64) - JS backend just makes all an F64 and accepts the precision loss, more for future Wasm backend.

## Wasm

Broader Wasm maturity beyond the current experimental path.

### Notes and limitations from previous investigations
- The WASM backend can't handle Choice/Union types yet (maps to Handle but produces i32/i64 mismatches). 
- Exponents (requires imported Math library)
- rt_string_from_i64 Wasm helper: Explicitly noted in the 1ac2613 commit message as an "incremental bridge implementation". It produces valid output but is not a complete runtime implementation. This is scoped for a dedicated follow-up and does not cause panics.

## Rust Interpreter
- Unimplmented - mostly just scaffolding
- primary goal: for basic CTFE as a release mode optimization step after HIR generation
- long term goal (noted here to not forget the idea): will work like MIRI to enable a special `checked: .. ;` blocks.
In beanstalk these would HAVE to be fully evaluated (would not be actually unsafe, just more heavily verified), but would run this additional advanced checking through those blocks specifically. Tradeoff of slower compile-times for using these special blocks, but gain more control. Rust interpreter means Beanstalk can do much more sophisticated analysis to prove the block is safe, allowing more flexible code patterns and faster runtime code.
