# Beanstalk Roadmap
This is the main todo list for the language and compiler.

The current major goal is getting to a healthy alpha stage.
Each plan or PR that is needed will be linked here.

Use the language surface integration matrix as a reference for what is currently implemented: `docs/src/docs/progress/#page.bst`

AST optimisation benchmark log: `docs/roadmap/refactors/ast-pipeline-optimisation-benchmark-log.md`

---

# Plans / Notes / TODOS
- Type environment redesign plan: `docs/roadmap/plans/type-environment-redesign-plan.md`
- Diagnostics refactor: `docs/roadmap/plans/compiler_diagnostics_redesign_plan.md`
- Generics (FINISHED PHASE 1 - 3 ONLY): `docs/roadmap/plans/beanstalk-generics-implementation-plan.md`
- Traits
- New Result/Error syntax: `docs/roadmap/plans/beanstalk-error-catch-syntax-migration-plan.md`
- True Results / Options with generics
- Parallel tokenization/header parsing and string table plan (after the major AST refactoring plans): `docs/roadmap/plans/parallel-tokenize-header-parse-string-table-plan.md`
- Closures
- Hash Maps
- Compile time arbitary precision aritmetic + Decimals Type support
- Move to more specific explicit type declarations for numbers (I32, I64, F32, F64) - JS backend just makes all an F64 and accepts the precision loss, more for future Wasm backend
- External non-scalar constant design: string slices, collections, and opaque-type external constants in const contexts are rejected for Alpha. Design compile-time representation and validation before enabling.
- `bean new` follow-ups: non-interactive `--yes`, template selection, project type aliases, richer scaffold presets, and optional package/dev tooling setup.

## Wasm

Broader Wasm maturity beyond the current experimental path.

### Notes and limitations from previous investigations
- The WASM backend can't handle Choice/Union types yet (maps to Handle but produces i32/i64 mismatches). 
- Exponents (requires explicit imported core math support)
- rt_string_from_i64 Wasm helper: Explicitly noted in the 1ac2613 commit message as an "incremental bridge implementation". It produces valid output but is not a complete runtime implementation. This is scoped for a dedicated follow-up and does not cause panics.
