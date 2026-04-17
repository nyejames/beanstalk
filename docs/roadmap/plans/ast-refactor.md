### REVERT MISTAKEN AST DRIFT

# Finalize, audit, document, and clean tests

## Overview

The goal is to finish the refactor cleanly and lock in the final AST shape.

This phase is about removing transitional structure, making the module layout obvious, tightening ownership boundaries inside the AST, and cleaning up tests/docs so they reflect the final architecture rather than the refactor process.

This phase is **not** the broader header/AST responsibility cleanup pass. Larger ownership cleanups such as `import_bindings.rs` and wider stage-boundary review will happen in a separate task. This phase should first finish the current refactor properly.

Desired design:

* `orchestrate.rs` is removed
* `ast/mod.rs` is the strict entry point and overview of the AST module
* `ast/mod.rs` clearly shows the AST pipeline flow and where the important parts live
* `ast/mod.rs` does not act as a transitional forwarding layer
* aliasing and old path shims are removed
* the public surface of `ast/mod.rs` is intentionally small and reflects the final architecture only
* `ScopeContext` and the main AST build context have clear responsibilities and are passed through the AST in a disciplined way
* touched code follows the style guide and compiler design docs
* tests reflect the current AST ownership model and pipeline, not the previous transitional structure

Done so far:

* the top-level template rewrite already removed a large amount of fragment-specific complexity
* the remaining work is now mostly structural cleanup, module-surface cleanup, documentation alignment, and test/lint follow-through

## Work to do

### 0. Remove `orchestrate.rs` and move its logic into `ast/mod.rs`

Files:

* `src/compiler_frontend/ast/module_ast/orchestrate.rs`
* `src/compiler_frontend/ast/mod.rs`

Finish phase 0 of this plan fully.

`orchestrate.rs` should be removed entirely once its logic has been moved into `ast/mod.rs`.

The final `ast/mod.rs` should be the first file someone reads to understand:

* the AST stage entry point
* the real pass order
* what the AST owns
* what it consumes from header parsing
* which internal files implement the important parts of the pipeline

Do not leave behind a split entrypoint.

### 1. Make `ast/mod.rs` the strict entry point and module overview

Files:

* `src/compiler_frontend/ast/mod.rs`
* `src/compiler_frontend/mod.rs`

Rewrite `ast/mod.rs` so it reflects the final architecture directly.

It should:

1. expose the AST stage entry point
2. show the real pipeline in order
3. act as the overview/orchestration file for the module
4. point clearly to the important internal files and types
5. contain concise comments describing the stage flow and responsibilities

It should not:

* preserve old forwarding structure
* reintroduce `orchestrate.rs` in another form
* expose internal details unnecessarily
* carry stale wording from the transitional refactor shape

### 2. Trim the public surface of `ast/mod.rs` after the refactor

Files:

* `src/compiler_frontend/ast/mod.rs`
* `src/compiler_frontend/ast/module_ast/mod.rs`
* any touched AST submodules

Once the final flow is in place, do a cleanup pass over visibility.

This should include:

* removing aliasing and old path shapes
* removing transitional re-exports
* reducing `pub` visibility for internal-only helpers, types, and modules
* keeping only the API surface needed by the rest of the compiler
* making the module root reflect the final architecture rather than the history of the refactor

The AST module should have one obvious public entry surface and minimal internal leakage.

### 3. Tighten context ownership and data-passing discipline inside the AST

Files:

* `src/compiler_frontend/ast/module_ast/build_state.rs`
* `src/compiler_frontend/ast/module_ast/scope_context.rs`
* any main AST build context type used by the refactor

Review the responsibilities and passing discipline of `ScopeContext` and the main AST build context.

The goal is to avoid context drift where state becomes split across overlapping structs without a clear reason.

This pass should check:

* what data belongs in long-lived AST build state
* what data belongs in local scope-tracking only
* which data should be passed explicitly instead of being stored broadly
* whether any fields are duplicated, partially overlapping, or only exist because of the transition
* whether naming still reflects the final role of each context clearly

Prefer one clear owner for each category of data.

Avoid multiple context structs carrying near-duplicate state or becoming generic “bags of stuff”.

### 4. Review the touched areas against the style guide and compiler design docs

Files:

* all files touched by this plan
* `docs/compiler-design-overview.md`
* `docs/codebase-style-guide.md`

Do a deliberate alignment pass:

* stage ownership matches the design overview
* `mod.rs` files reflect the intended organisation rules
* module boundaries remain clear
* no transitional wrappers or compatibility shims remain
* comments explain behavior and rationale, not syntax
* error paths use structured diagnostics and avoid user-input panics
* naming stays explicit and full
* files and functions still have one clear responsibility

### 5. Add strong comments to the new AST pipeline key parts

Files:

* `src/compiler_frontend/ast/mod.rs`
* `src/compiler_frontend/ast/module_ast/build_state.rs`
* `src/compiler_frontend/ast/module_ast/scope_context.rs`
* `src/compiler_frontend/ast/import_bindings.rs`
* `src/compiler_frontend/ast/module_ast/pass_emit_nodes.rs`

The key AST pipeline parts should be clearly commented.

Comments should explain:

* what this stage owns
* what it no longer owns
* why `ast/mod.rs` is the strict entry point
* how header parsing and AST responsibilities are currently divided
* how AST uses the shared top-level manifest
* how local scope growth works during body lowering
* how the surviving build/scope contexts relate to each other
* how entry `start()` and const fragment finalization relate to the big picture

Use concise WHAT/WHY comments and file-level docs.

### 6. Clean remaining lints, dead code, and stale tests

This final phase must explicitly include:

* cleaning up any remaining `clippy` lints in touched areas
* reviewing dead code, stale `#[allow(dead_code)]`, unused helpers, and leftover unused paths
* removing stale comments and docs from the old architecture
* removing leftover legacy codepaths or aliases kept only during transition
* updating or deleting tests that still assert transitional AST structure instead of the final model
* running the required checks from the style guide:

  * `cargo clippy`
  * `cargo test`
  * `cargo run tests`

## Done when

* `orchestrate.rs` is gone
* `ast/mod.rs` is the strict AST entry point and overview file
* aliasing, transitional re-exports, and old path shims are removed
* the public surface of the AST module is intentionally small and reflects the final architecture only
* `ScopeContext` and the main AST build context have clear, non-overlapping responsibilities
* touched code follows the style guide and compiler design docs
* key AST pipeline files are well commented with clear WHAT/WHY and stage ownership
* tests validate the final architecture rather than transitional internals
* remaining clippy lints in touched areas are cleaned up
* no stale architectural comments from the old model remain