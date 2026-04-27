# External package registry organisation and cleanup plan

This plan covers the next cleanup pass after the initial `@std/math` implementation.

The goal is to get ahead of external package growth before more standard packages or platform APIs are added.

## Goal

Make external package infrastructure easier to extend, cheaper to pass around, and less likely to accumulate churn.

This should leave the current Alpha behavior intact while improving code organisation and implementation quality.

## Context

The external package registry now owns more than host function metadata:

- package-scoped external functions
- opaque external types
- compile-time external constants
- prelude symbols
- ABI metadata such as `I32`, `F64`, `Utf8Str`, and `Handle`
- JS lowering metadata for runtime helpers
- test-only synthetic packages
- builtin package definitions such as `@std/io`, `@std/collections`, `@std/error`, and `@std/math`

This is the right direction, but the current implementation is reaching the point where one file can become a junk drawer.

The cleanup should happen before adding larger APIs such as canvas, filesystem, time, random, DOM, or richer standard-library packages.

## Non-goals

This pass should not redesign the external package system.

Do not add:

- user-authored external binding files
- a new package manifest format
- dynamic runtime package loading
- Wasm external package support beyond preserving current experimental behavior
- `InlineExpression` JS lowering unless it is needed to remove real complexity
- larger standard-library APIs

If a bug is found, fix it. Otherwise, this is an organisation and quality pass.

## Desired end state

The external package system should have a clearer shape:

```text
src/compiler_frontend/external_packages/
├── mod.rs
├── ids.rs
├── abi.rs
├── definitions.rs
├── registry.rs
├── builtin_packages.rs
└── packages/
    ├── mod.rs
    ├── std_io.rs
    ├── std_collections.rs
    ├── std_error.rs
    ├── std_math.rs
    └── test_packages.rs
```

The exact filenames can change if the implementation suggests a better split.
The important rule is that package definitions should not be mixed with general registry mechanics.

## Work stages

### 1. Split registry types from package registration

Move low-level data shapes into focused files:

- stable IDs
- ABI types
- access and alias metadata
- function/type/constant definitions
- JS/Wasm lowering metadata
- package and registry structs

Keep `mod.rs` as a map of the module, not a dumping ground.
It should explain the data flow and re-export the public surface.

### 2. Split builtin package definitions

Move package construction into package-specific helpers.

Expected helpers:

- `register_std_io_package`
- `register_std_collections_package`
- `register_std_error_package`
- `register_std_math_package`
- `register_test_packages_for_integration`

The registry constructor should read like orchestration:

```rust
pub fn new() -> Self {
    let mut registry = Self::default();
    register_std_io_package(&mut registry)?;
    register_std_collections_package(&mut registry)?;
    register_std_error_package(&mut registry)?;
    register_std_math_package(&mut registry)?;
    registry
}
```

Use the actual error-handling shape that fits the codebase. Do not panic on user-controlled input. Builtin registration may still use invariant panics if they remain clearly impossible and tied to compiler-owned definitions.

### 3. Audit clone usage

Look for clones added by the external package path and decide whether each one is necessary.

Focus areas:

- `ExternalPackageRegistry` passing into frontend contexts
- JS lowering config and emitter construction
- package definitions copied into lookup maps
- `ExternalFunctionDef`, `ExternalTypeDef`, and `ExternalConstantDef` storage
- visible external symbol maps
- test package registration

Possible improvements:

- use references where lifetimes are already natural
- store IDs in visibility maps, not full definitions
- avoid cloning registries during backend lowering when a borrow is enough
- avoid cloning package definitions into both package maps and ID maps if one side can store IDs
- prefer small copyable IDs across stages

Do not make the code lifetime-hostile for a small clone win. Readability remains the first priority.

### 4. Tighten registry APIs

Make invalid operations hard to express.

Review whether public methods should be split into:

- package registration
- symbol registration
- ID lookup
- package-scoped lookup
- prelude lookup
- test-only helpers

Look for places where a method name is too broad, misleading, or duplicates another lookup path.

The import binder and expression/type resolution should continue to resolve through file-local visibility rather than global bare-name lookup.

### 5. Clean JS lowering integration

Review the JS backend integration after the registry split.

Keep these rules:

- HIR stores stable external function IDs, not JS runtime names
- JS lowering resolves IDs through registry lowering metadata
- standard runtime helpers are emitted only when referenced
- missing JS lowering metadata produces a compiler error, not silent fallback
- `InlineExpression` stays explicitly unsupported unless implemented deliberately

Also check that helper emission order remains safe and predictable.

### 6. Tidy comments and documentation

Clean up minor rough edges introduced during the first implementation pass.

Examples:

- remove duplicated comments
- keep comments grammatical and useful
- delete comments that only restate code
- add short WHAT/WHY comments where control flow is not obvious
- make `mod.rs` explain the external package module layout
- update this roadmap plan if the implementation direction changes materially

### 7. Validation and tests

This pass should preserve all existing external package behavior.

Run:

```bash
just validate
```

At minimum, run:

```bash
cargo clippy
cargo test
cargo run tests
```

Add tests only where the refactor reveals a missing edge.
Avoid test churn for file movement alone.

## Acceptance criteria

This plan is complete when:

- external package code is split into focused files or modules
- builtin package definitions are no longer embedded in a large registry constructor
- `@std/math` behavior is unchanged
- explicit import visibility still gates non-prelude external symbols
- external constants still work in runtime expressions and constant contexts
- JS external lowering still resolves through registry metadata
- unnecessary clones introduced by the registry path have been removed or consciously kept
- comments and module docs follow the style guide
- `just validate` passes, or any skipped command is recorded with a reason

## Follow-up candidates

After this cleanup, the next external-package expansion can be planned from a cleaner base.

Good follow-up candidates:

- `@std/random`
- `@std/time`
- `@web/canvas`
- `@web/dom`
- a deliberate `InlineExpression` lowering design
- a proper Wasm external import model
