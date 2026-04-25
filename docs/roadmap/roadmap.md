# Beanstalk Roadmap
This is the main todo list for the language and compiler.

The current major goal is getting to a healthy alpha stage.
Each plan or PR that is needed will be linked here.

Use the language surface integration matrix as a reference for what is currently implemented: `docs/src/docs/progress/#page.bst`

---

# Plans / Notes / TODOS

## Typed imported platform packages

Generalise the current hardcoded host function registry into a backend-neutral typed external package system.

Project builders provide virtual packages such as `@std/io`, `@web/canvas`, `@web/dom`, or Rust-backed host APIs. These packages expose typed functions, opaque external types, receiver methods, access/mutability metadata, return/error metadata, and backend lowering keys. This becomes the general mechanism for all builder-specific callable imports. Style directives remain separate because they affect template parsing and formatting rather than runtime semantics.

The selected project builder must provide required default packages such as `@std/io`. The compiler prelude-imports selected required symbols such as `io()`, preserving the current ergonomics while removing hardcoded host-call special cases.

### Design overview

1. **Rename/generalise `HostRegistry` into `ExternalPackageRegistry`**  
   `ExternalPackageRegistry` holds builder-provided packages keyed by path (e.g. `"@std/io"`). Each package contains `ExternalFunctionDef`, `ExternalTypeDef`, and `ExternalMethodDef` entries. `ExternalAbiType` is expanded to a Wasm-first set (`Void`, `Bool`, `I32`, `I64`, `F32`, `F64`, `StringSlice`, `StringOwned`, `Handle`).

2. **Required `@std/io` package**  
   Every `BackendBuilder` implements `external_packages()` and supplies `@std/io` containing the `io()` function. The compiler automatically makes `io` visible in every module without explicit imports.

3. **Virtual package imports**  
   `import @web/canvas { Canvas, Canvas2d }` resolves against the builder registry instead of the filesystem. Virtual imports do not create source-file dependency edges.

4. **Opaque external types**  
   Sealed nominal types (e.g. `Canvas`, `FileHandle`) can be passed, returned, and used as receivers, but cannot be constructed with struct literals, field-accessed, or pattern-matched.

5. **External receiver methods**  
   Platform packages define methods on external types with `Shared` or `Mutable` receiver access. Mutable receivers require `~receiver.method(...)`.

6. **Stable external function IDs in HIR**  
   `CallTarget::ExternalFunction(ExternalFunctionId)` replaces stringly `HostFunction` paths. Backends map the same ID to their own lowering key (JS runtime name, Wasm import, Rust host binding).

7. **Wasm-first ABI lowering**  
   Primitives lower directly. Strings use UTF-8 pointer+length. Opaque objects use `i32` handles. JS output remains compatible with this shape.

8. **`@web/canvas` validation package**  
   The first substantial platform package, exercising opaque handles, mutable receivers, floats, loops, and JS runtime glue.

### Implementation phases

- **Phase 1** — Rename registry and types (`HostRegistry` → `ExternalPackageRegistry`, etc.). Thread through all compiler stages. *(Completed)*
- **Phase 2** — Add `BackendBuilder::external_packages()`. Move `io()` into the default `@std/io` package. Preserve prelude visibility. *(Completed)*
- **Phase 3** — Support `@package/path` import syntax. Resolve virtual imports against the builder registry. *(Completed)*
- **Phase 4** — Add `ExternalTypeDef`, `ExternalTypeId`, and `DataType::External`. Reject construction/field-access.
- **Phase 5** — Add `ExternalMethodDef`. Resolve external receiver methods in AST. Enforce mutability.
- **Phase 6** — Replace `CallTarget::HostFunction(InternedPath)` with `CallTarget::ExternalFunction(ExternalFunctionId)`. Flatten metadata for backend lookup.
- **Phase 7** — Expand `ExternalAbiType` to full Wasm set. Define ABI lowering tables in JS and Wasm backends.
- **Phase 8** — Implement `@web/canvas` in the HTML builder. Write integration tests.

### Non-goals

- User-authored binding files (`.bst` bindings)
- `extern js` / `extern wasm` / `extern rust` syntax
- Merging style directives with platform packages
- Compatibility shims for the old host registry

### Acceptance criteria

- `io()` works via `@std/io` without hardcoded frontend special cases.
- Virtual package imports resolve and type-check.
- Opaque external types cannot be constructed or field-accessed.
- External receiver methods work with shared and mutable receivers.
- HIR uses stable `ExternalFunctionId` instead of stringly host names.
- JS and Wasm backends map the same IDs differently.
- `@web/canvas` integration tests pass.
- `just validate` passes after every phase.

---

- builtin `Error` enrichment beyond what is already required for the current compiler/runtime surface
- full tagged unions
- full pattern-matching design (capture patterns)
- full traits implementation
- Closures
- Hash Maps
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
