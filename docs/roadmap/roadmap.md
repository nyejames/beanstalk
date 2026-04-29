# Beanstalk Roadmap
This is the main todo list for the language and compiler.

The current major goal is getting to a healthy alpha stage.
Each plan or PR that is needed will be linked here.

Use the language surface integration matrix as a reference for what is currently implemented: `docs/src/docs/progress/#page.bst`

---

# Plans / Notes / TODOS
- Library system: `docs/roadmap/plans/core-and-builder-library-implementation-plan.md`
  - Deferred: package manager, library versions, remote fetching, dependency lockfiles, library override/shadowing, source-library HIR caching, user-authored external binding files, namespace/wildcard imports, seeded random, full date/time/timezone APIs, and Wasm support for non-math core packages.

- Pattern matching hardening: 
Finish coverage and diagnostics for the supported Alpha pattern surface; keep general capture/tagged patterns, nested payload patterns and negated patterns

- full traits implementation
- Closures
- Hash Maps
- Compile time arbitary precision aritmetic + Decimals Type support
- Move to more specific explicit type declarations for numbers (I32, I64, F32, F64) - JS backend just makes all an F64 and accepts the precision loss, more for future Wasm backend

## Wasm

Broader Wasm maturity beyond the current experimental path.

### Notes and limitations from previous investigations
- The WASM backend can't handle Choice/Union types yet (maps to Handle but produces i32/i64 mismatches). 
- Exponents (requires explicit imported core math support)
- rt_string_from_i64 Wasm helper: Explicitly noted in the 1ac2613 commit message as an "incremental bridge implementation". It produces valid output but is not a complete runtime implementation. This is scoped for a dedicated follow-up and does not cause panics.
