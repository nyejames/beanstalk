# Beanstalk Roadmap
This is the main todo list for the language and compiler.

The current major goal is getting to a healthy alpha stage.
Each plan or PR that is needed will be linked here.

Use the language surface integration matrix as a reference for what is currently implemented: `./language-surface-integration-matrix.md`

## Path to Alpha
These are the non-negotiable conditions for starting Alpha:

- All claimed Alpha features compile, type check, and run through the full supported pipeline.
- Unsupported syntax or incomplete features fail with structured compiler diagnostics, not panics.
- The integration suite covers the supported language surface, not just recent feature areas.
- The JS backend and HTML builder are stable enough for real small projects and docs-style sites.
- Compiler diagnostics are useful, accurate, consistently formatted, and visually moving toward the Nushell-style goal.
- Cross-platform output is stable enough that Windows and macOS do not produce avoidable golden drift.
- The documentation site (written in beanstalk) inside the docs directory should be able to render a complete and good looking docs website fully using the Beanstalk pipeline. This will be the final testing ground for whether the language feels "ready" to be alpha.

## Next Plans
- 2 remaining windows test failures `docs\roadmap\plans\windows-failure.md`


### JS backend jump argument lowering

Complete JS backend lowering for HirTerminator::Jump.args

The HIR already models Jump { target, args }, but the JS backend still rejects non-empty jump argument lists during both structured and dispatcher lowering. This is a backend contract gap between the HIR shape and JS codegen, and it will become more limiting as CFG lowering becomes more explicit and block transitions carry values directly through the graph  ￼  ￼  ￼

Todo

* Add full JS lowering support for non-empty HirTerminator::Jump.args
* Define the JS-side block-parameter strategy clearly so value flow across blocks is explicit and deterministic
* Support this in both structured lowering and dispatcher lowering, or explicitly constrain structured lowering to only the subset that can safely carry block arguments
* Remove current backend hard-errors for non-empty jump args
* Add focused backend tests for:
    * simple block-to-block value passing
    * branch merge with block arguments
    * loop back-edge with block arguments
    * interaction with alias/reference locals
* Add at least one end-to-end compiler fixture that proves a real frontend program can lower through this path once the HIR starts using it

### Unify JS collection runtime semantics

Make JS collection helper semantics strict and consistent

The JS runtime currently treats collection operations inconsistently. __bs_collection_get returns structured Result errors for invalid collections and out-of-bounds access, while __bs_collection_push and __bs_collection_remove silently no-op and __bs_collection_length returns 0 for non-array inputs. This weakens the backend contract and can hide user bugs behind permissive JS behavior  ￼

Todo

* Decide one explicit semantic policy for JS collection helpers and document it
* Make push, remove, and length follow the same error/contract style as get
* Avoid silent no-ops for invalid collection operations unless this is explicitly part of the language semantics
* Ensure runtime error values use the canonical builtin error shape and codes where appropriate
* Add integration tests for:
    * invalid receiver type for each collection helper
    * out-of-bounds remove
    * invalid index type
    * length on invalid receiver
    * success-path behavior for all collection helpers
* Add backend contract tests that pin emitted JS helper behavior so this does not drift later

### Add integration tests for frontend rejection of invalid string coercion

Todo

* Add fixtures showing AST rejects invalid automatic string coercion for:
    * struct values
    * collection values
    * option/result carriers
    * tuples/multiple returns where not explicitly allowed
* Add template-head and template-body variants
* Add one regression test that proves valid string-slice/template paths still compile while invalid value types fail early


## JS backend extension

### Expand JS backend and runtime coverage for choices

The matrix already marks choices as implemented but incomplete, with thin backend-specific coverage  ￼

Todo

* Add JS backend contract tests for lowering choice construction and choice matching
* Add integration fixtures covering choice values flowing through:
    * function returns
    * assignment
    * nested match expressions/statements
    * template/control-flow boundaries where relevant
* Add emitted-JS tests that pin the runtime carrier shape used for current choice lowering
* Add negative integration cases for unsupported deferred choice sub-surfaces so deferred behavior stays intentional and stable
* Add backend/runtime cases for cross-file exported choices to ensure symbol resolution and lowering remain aligned

### Expand JS backend and runtime coverage for pattern matching

The matrix marks pattern matching as implemented but incomplete, with deferred richer pattern forms and relatively lighter backend/runtime hardening than the frontend surface deserves  ￼

Todo

* Add JS backend tests for literal match lowering in both:
    * structured lowering path
    * dispatcher fallback path
* Add integration cases for:
    * match in loops
    * nested match in branches
    * match returning values through merges
    * match with guards
    * wildcard merge behavior
* Add tests proving emitted JS merge behavior is stable when arms converge on a continuation block
* Add regression tests for “no arm selected” behavior where the frontend should have guaranteed exhaustiveness
* Add more adversarial cases combining match with results/options and control-flow-heavy code

### Expand backend/runtime coverage for receiver methods outside the current happy path

The matrix calls receiver methods implemented, but HTML / HTML-Wasm specific runtime cases are still light, and more backend-facing receiver/field mutation cases are still useful  ￼

Todo

* Add JS integration tests for immutable and mutable receiver methods on:
    * structs
    * nested structs
    * scalar receivers where supported
* Add cases for chained receiver calls mixed with field reads/writes
* Add tests covering aliasing-sensitive receiver cases so the binding model is pinned under method syntax
* Add emitted-JS tests for receiver calls that return aliases vs fresh values
* Add backend regression cases for exported receiver methods across files

### Strengthen collection backend/runtime coverage beyond current basics

The matrix already has broad collection coverage, but backend/runtime contract hardening is still worth deepening, especially around edge behavior  ￼

Todo

* Add JS contract tests for all collection helpers, not just get
* Add integration cases for:
    * invalid receiver type
    * invalid index type
    * negative index
    * mutation through alias/reference paths
    * indexed write followed by readback
    * mutation inside loops and branches
* Add regression fixtures proving explicit mutable access requirements are preserved all the way through emitted JS


### Harden JS result/error runtime coverage further

The backend already has dedicated helpers and some tests, but more integrated adversarial cases are still useful  ￼  ￼  ￼

Todo

* Add integration fixtures for nested ! propagation through multiple function layers
* Add fallback-path cases inside loops, branches, and match arms
* Add emitted-JS tests pinning error trace and bubble behavior in more than the minimal helper-level contract
* Add cases mixing result propagation with runtime fragment generation/templates

### Add explicit backend coverage for block-dispatcher edge cases

The dispatcher path is important and should be hardened more aggressively because it is the fallback for nontrivial CFG  ￼  ￼

Todo

* Add more dispatcher-only integration fixtures for:
    * nested loops
    * loop + match
    * break/continue chains
    * branch-heavy cyclic CFG
* Add regression tests proving structured lowering is chosen when legal and dispatcher lowering only when needed
* Add future tests for jump-arg lowering once implemented


### Some basic optimisation work over ast. Major bottleneck function is: parse_function_body_statements(). THis is where the compiler is spending the vast majority of the total compile time.

### Move to more specific explicit type declarations for numbers (I32, I64, F32, F64) - JS backend just makes all an F64 and accepts the precision loss, more for future Wasm backend.

### Review built in "Error" type and reserved keywords
Should this be build-system provided (like IO) rather than a compiler built in? So Error is reserved in a similar way to io and IO, and must always be provided by the build system, but the specific shape beyond the core parameters must be defined by the build system.



## Final pre-alpha sweep

- Re-run the feature matrix and mark all supported areas as covered.
- Re-check that unsupported/deferred features fail cleanly.
- Re-check that docs and examples match actual support.
- Re-check diagnostics quality on a representative set of failures.
- Re-check cross-platform golden stability.

### Alpha cleanup

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

## Wasm

Broader Wasm maturity beyond the current experimental path.

### Notes and limitations from previous investigations
- The WASM backend can't handle Choice/Union types yet (maps to Handle but produces i32/i64 mismatches). 
- Exponents (requires imported Math library)
- rt_string_from_i64 Wasm helper: Explicitly noted in the 1ac2613 commit message as an "incremental bridge implementation". It produces valid output but is not a complete runtime implementation. This is scoped for a dedicated follow-up and does not cause panics.

## Rust Interpreter
- Unimplmented - mostly just scaffolding
- Make sure Modulus is Eulidean
