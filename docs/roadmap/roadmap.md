# Beanstalk Roadmap

This catalogues the todo list for the language and compiler.

The current major goal is getting to a healthy alpha stage.
Each plan or PR that is needed will be linked here.

Use the language surface integration matrix as a reference for what is currently implemented: `./language-surface-integration-matrix.md`

## Path to Alpha

These are the non-negotiable conditions for starting Alpha.

- All claimed Alpha features compile, type check, and run through the full supported pipeline.
- Unsupported syntax or incomplete features fail with structured compiler diagnostics, not panics.
- The integration suite covers the supported language surface, not just recent feature areas.
- The JS backend and HTML builder are stable enough for real small projects and docs-style sites.
- Compiler diagnostics are useful, accurate, consistently formatted, and visually moving toward the Nushell-style goal.
- Cross-platform output is stable enough that Windows and macOS do not produce avoidable golden drift.
- The documentation site (written in beanstalk) inside the docs directory should be able to render a complete and good looking docs website fully using the Beanstalk pipeline. This will be the final testing ground for whether the language feels "ready" to be alpha.

## Next Plans

- `docs/roadmap/plans/mutable-literal-mutable-params-hidden-locals-plan.md`
Support fresh rvalues directly in mutable (`~T`) function-parameter slots by lowering them through synthesized hidden locals in HIR. Keeps `~` place-only, keeps `~literal` invalid, avoids adding a new HIR node kind, and extends tests/docs for the new call-site rule.

- `docs/roadmap/plans/js-backend-hardening.md`
Reviewing the JS backend and making sure it implements the full suite of alpha features.

- `docs/roadmap/plans/cross-platform-compat.md`
Some tests current fail on windows, but the language is still usable.
This is due to things like CRLF in golden outputs and OS path drifts.

### Additional TODOs
- Explicit error for compile time number overflows (2 ^ 63) should not just be a rust panic, should be a graceful compile time error.
- Loops can accept a single integer as a condition and it will automatically create a random from 0 to {integer}.
- Move to more specific explicit type declarations for numbers (I32, I64, F32, F64) - JS backend just makes all an F64 and accepts the precision loss, more for future Wasm backend. 
- Review built in "Error" type and reserved keywords. Should this be build-system provided (like IO) rather than a compiler built in? So Error is reserved in a similar way to io and IO, and must always be provided by the build system, but the specific shape beyond the core parameters must be defined by the build system.

## Final pre-alpha sweep

### PR - Alpha checklist audit

Verify that the Alpha gates are genuinely met.

**Checklist**
- Re-run the feature matrix and mark all supported areas as covered.
- Re-check that unsupported/deferred features fail cleanly.
- Re-check that docs and examples match actual support.
- Re-check diagnostics quality on a representative set of failures.
- Re-check cross-platform golden stability.

**Done when**
- There is a credible yes/no answer to “is Alpha ready?”

### PR - Alpha cleanup PR

Land final small consistency and hygiene fixes before the release branch/tag.

**Checklist**
- Remove obsolete rejection fixtures for features that are now supported.
- Update mod.rs files to follow the compiler style guide and refactor modules to make sure they are following `docs/codebase-style-guide`.
- Tighten comments, TODOs, and dead-code justifications.
- Prune stale scaffolding where the current design has clearly replaced it.
- Update release-facing docs and contribution notes if needed.

**Done when**
- The repo feels intentional at the point Alpha begins.

---

## Deferred until after Alpha
These are intentionally not Alpha blockers unless they become necessary for one of the supported slices.

This is a collection of notes and findings for future roadmaps once the roadmap above is complete.

- builtin `Error` enrichment beyond what is already required for the current compiler/runtime surface
- full tagged unions
- full pattern-matching design
- full interfaces implementation
- richer numeric redesign work not required by Alpha
- Compile time arbitary precision aritmetic + Decimals Type support

**Wasm**

Broader Wasm maturity beyond the current experimental path.

## Notes and limitations from previous investigations
- The WASM backend can't handle Choice/Union types yet (maps to Handle but produces i32/i64 mismatches). 
- rt_string_from_i64 Wasm helper: Explicitly noted in the 1ac2613 commit message as an "incremental bridge implementation". It produces valid output but is not a complete runtime implementation. This is scoped for a dedicated follow-up and does not cause panics.
