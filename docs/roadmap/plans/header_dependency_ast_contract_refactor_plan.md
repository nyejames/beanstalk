# Beanstalk Header / Dependency Sorting / AST Contract Refactor Plan

## Purpose

This plan fixes the contract between header parsing, dependency sorting, and AST construction.

The intended frontend flow is:

```text
Tokenization
  -> Header parsing / import preparation / declaration shell creation
  -> Dependency sorting of all top-level declaration dependencies
  -> AST linear declaration resolution and body emission
```

After dependency sorting, AST should walk sorted headers in order. It should not build another top-level ordering graph, rediscover imports, or reconstruct top-level declaration shells from raw tokens.

This plan also fixes the current code-quality risk around import binding code. The existing AST-side import code must not be copied into the header stage unchanged. The move must improve structure, names, APIs, comments, diagnostics, and stage ownership.

## Design rule

> If AST cannot resolve a top-level declaration by walking sorted headers in order, the missing dependency belongs in header parsing or dependency sorting, not in a new AST ordering pass.

## Current problem

The repo has drifted into duplicate and misplaced work:

- `src/compiler_frontend/module_dependencies.rs` already performs top-level dependency sorting.
- `src/compiler_frontend/ast/module_ast/environment/constant_graph.rs` performs a second topological sort for constants.
- `src/compiler_frontend/ast/import_bindings.rs` owns import binding and file-local visibility construction, even though this belongs in header/import preparation.
- `src/compiler_frontend/ast/module_ast/environment/import_environment.rs` wires AST environment import state with noisy, argument-heavy APIs.
- `src/compiler_frontend/headers/header_dispatch.rs` already parses constant declaration shells and collects initializer references, but constant initializer references are not yet first-class dependency edges.
- `src/compiler_frontend/headers/file_parser.rs` already separates imports, top-level declarations, top-level const fragments, and entry `start` body tokens, but the parsed import data is not yet prepared into the contract AST should consume.

This plan moves the missing ownership to the correct stage and deletes AST-side compensating systems.

## Non-goals

This plan does **not** redesign the full `DataType` / type-system representation.

This plan does **not** move function/start executable body parsing into headers.

This plan does **not** rewrite expression parsing or constant folding. AST still parses and folds constant initializer expressions semantically.

This plan does **not** keep compatibility shims for old AST import binding or AST constant graph flows. Beanstalk is pre-alpha. When ownership moves, old paths should be removed.

This plan does **not** parallelize header parsing. Header-stage parallel parsing is a follow-up after this contract is stable.

## Documentation precondition

Before implementing this plan, the following docs should already reflect the intended contract:

```text
docs/compiler-design-overview.md
docs/codebase-style-guide.md
```

This plan assumes:

- header/import preparation owns file-local visibility construction
- header/import preparation owns import alias resolution
- header/import preparation owns facade/re-export preparation
- header/import preparation owns constant initializer dependency edge extraction
- dependency sorting owns the only top-level declaration sort
- AST consumes sorted headers and the header-built import environment
- AST does not build import visibility
- AST does not sort constants
- moved code must improve ownership, names, APIs, comments, and data flow

If implementation work makes these docs inaccurate, update the docs in the same phase.

## Required benchmark and validation gate

Every code-changing phase must update the benchmark log and end green.

Before implementation of each phase:

```bash
just bench
```

During implementation, optional directional checks:

```bash
just bench-quick
```

After implementation of each phase:

```bash
just bench
just validate
```

Each phase commit must include:

- code changes
- benchmark log update with before/after `just bench` summary paths and key values
- relevant documentation/comment updates if the implementation changed public architecture
- phase audit notes
- readability/code-shape audit notes

Generated files in `benchmarks/results/` must not be committed. Commit only summarized results in the benchmark log.

## Benchmark log

Use:

```text
docs/roadmap/refactors/ast-pipeline-optimisation-benchmark-log.md
```

If the file does not exist yet, create it in Phase 0.

Add a section named:

```text
Header / Dependency Sorting / AST Contract Refactor
```

Each phase entry should include:

- phase name
- before benchmark run directory / summary path
- after benchmark run directory / summary path
- key benchmark rows: check/build speed-test, check docs, relevant stress cases
- relevant detailed timer changes: headers, dependency sorting, AST/build environment, AST/constants
- regression classification
- notes / follow-up optimization observations

Regression classification:

```text
Improved:          >= 3% faster
Neutral:           within ±3%
Regression:        >= 3% slower
Major regression:  >= 10% slower
```

A major regression blocks continuation unless the cause is identified and explicitly accepted.

## Required code-quality gate

This refactor must not copy AST-side compensating code into the header stage unchanged.

Any code moved from:

```text
src/compiler_frontend/ast/import_bindings.rs
src/compiler_frontend/ast/module_ast/environment/import_environment.rs
src/compiler_frontend/ast/module_ast/environment/constant_graph.rs
```

must be reshaped while moving so the new owner is clearer than the old one.

Minimum requirements:

- No new header-stage module may become a dumping ground for import, re-export, facade, external package, constant, and AST semantic behavior.
- Large free functions with long parameter lists must be replaced with focused context/input structs.
- Any function with more than five non-trivial parameters requires a local context/input struct unless the phase audit explains why not.
- Do not add `#[allow(clippy::too_many_arguments)]` to new or moved code.
- Prefer narrow modules with one responsibility over one large `import_environment.rs`.
- `mod.rs` files should be structural maps: orchestration, re-exports, and module docs only.
- File-level docs must state what the module owns, why it exists, and what it must not do.
- Add concise WHAT/WHY comments for complex blocks, invariants, control-flow joins, and non-obvious failure cases.
- Do not add comments that merely restate syntax.
- Extract repeated diagnostic construction into named helpers.
- Use enums for resolution/classification outcomes instead of boolean-heavy APIs.
- Avoid compatibility wrappers for old AST ownership paths.
- Delete old code paths once the new header-owned path is wired.

Each phase must include this readability/code-shape audit:

```text
- Which old module/function was removed or split?
- Which new modules now own the behavior?
- Which long argument lists were replaced by context/input structs?
- Which duplicated logic was deleted instead of wrapped?
- Are any too-large functions, large match blocks, or mixed-responsibility modules still present?
- Are comments helpful and specific, or do they merely restate syntax?
- Do all touched files follow docs/codebase-style-guide.md?
- Is any cleanup intentionally deferred? If yes, where is the follow-up recorded?
```

## Target stage contract

### Header parsing / import preparation owns

Header parsing and header preparation do one cheap structural pass over each file, then a module-level import/symbol preparation step.

It owns:

- import and re-export syntax parsing
- import path validation and normalization where enough context exists
- import alias parsing and validation
- facade import/re-export preparation
- external package import preparation
- file-local visibility environment construction
- visible-name collision checks
- builtin/prelude reservation checks
- top-level declaration discovery
- declaration shell parsing for constants, functions, structs, choices, type aliases, const templates, and entry start headers
- generic parameter syntax attached to declarations and type surfaces
- constant initializer token capture
- constant initializer reference hint collection
- constant initializer dependency edge generation
- all dependency edge generation needed before AST can resolve declarations linearly
- entry `start` body capture and separation from dependency-sorted headers
- top-level const fragment placement metadata
- `ModuleSymbols` construction and final header-owned metadata needed by dependency sorting and AST

### Dependency sorting owns

Dependency sorting consumes header-provided dependency edges and produces sorted headers.

It owns:

- topological sorting of top-level declaration headers
- cycle detection for top-level declaration dependencies
- missing dependency diagnostics
- source-order stability among independent declarations
- same-file constant source-order enforcement
- appending entry `StartFunction` headers after sorted declarations
- finalizing `ModuleSymbols.declarations` in sorted order

Constant initializer references to other constants are top-level declaration dependencies. They are not executable body references.

### AST owns

AST consumes sorted headers and header-built visibility/declaration metadata.

It owns:

- linear declaration resolution from sorted shells
- type alias, constant, struct, choice, and function signature semantic validation
- constant folding and const-only validation
- expression parsing and type checking
- body-local declaration shell creation and full resolution through shared declaration syntax code
- function/start/template body parsing
- receiver method cataloging once signatures are resolved
- template semantic preparation and finalization
- HIR-facing AST assembly

AST must not:

- topologically sort constants or other top-level declarations
- rebuild file import bindings from scratch
- rediscover top-level symbols from raw file tokens
- reconstruct top-level declaration shells from raw tokens
- add an ordering pass to compensate for missing header dependency edges

---

# Target API shapes

These APIs are examples and should guide the implementation. Exact field names can change, but the ownership and shape should not drift.

## Header output

```rust
pub struct Headers {
    pub headers: Vec<Header>,
    pub top_level_const_fragments: Vec<TopLevelConstFragment>,
    pub entry_runtime_fragment_count: usize,
    pub module_symbols: ModuleSymbols,
    pub import_environment: HeaderImportEnvironment,
    pub warnings: Vec<CompilerWarning>,
}
```

## Sorted header output

```rust
pub(crate) struct SortedHeaders {
    pub(crate) headers: Vec<Header>,
    pub(crate) top_level_const_fragments: Vec<TopLevelConstFragment>,
    pub(crate) entry_runtime_fragment_count: usize,
    pub(crate) module_symbols: ModuleSymbols,
    pub(crate) import_environment: HeaderImportEnvironment,
    pub(crate) warnings: Vec<CompilerWarning>,
}
```

`SortedHeaders` is the AST-facing contract. AST should receive this or a direct projection of it, not a loose pile of parameters.

## Header import environment

Rename `FileImportBindings` to `FileVisibility`.

Reason: after this refactor, the structure is not only "import bindings." It includes same-file declarations, source imports, external symbols, aliases, type aliases, prelude symbols, and builtins.

```rust
#[derive(Clone, Default)]
pub(crate) struct FileVisibility {
    pub(crate) visible_declaration_paths: FxHashSet<InternedPath>,
    pub(crate) visible_source_names: FxHashMap<StringId, InternedPath>,
    pub(crate) visible_type_alias_names: FxHashMap<StringId, InternedPath>,
    pub(crate) visible_external_symbols: FxHashMap<StringId, ExternalSymbolId>,
}
```

```rust
pub(crate) struct HeaderImportEnvironment {
    pub(crate) file_visibility_by_source: FxHashMap<InternedPath, FileVisibility>,
    pub(crate) warnings: Vec<CompilerWarning>,
}

impl HeaderImportEnvironment {
    pub(crate) fn visibility_for(
        &self,
        source_file: &InternedPath,
    ) -> Result<&FileVisibility, CompilerError> {
        // Return a compiler bug diagnostic if a parsed source file has no visibility entry.
        // Missing visibility means header preparation failed to populate its stage contract.
    }
}
```

## Import environment input

```rust
pub(crate) struct ImportEnvironmentInput<'a> {
    pub(crate) module_symbols: &'a mut ModuleSymbols,
    pub(crate) external_package_registry: &'a ExternalPackageRegistry,
    pub(crate) string_table: &'a mut StringTable,
}

pub(crate) fn prepare_import_environment(
    input: ImportEnvironmentInput<'_>,
) -> Result<HeaderImportEnvironment, CompilerMessages>;
```

This replaces long function signatures such as `resolve_file_import_bindings(...)`.

## Import environment module layout

Use a module directory, not one large file:

```text
src/compiler_frontend/headers/import_environment/
    mod.rs
    bindings.rs
    visible_names.rs
    target_resolution.rs
    facade_resolution.rs
    re_exports.rs
    diagnostics.rs
```

Suggested ownership:

```text
mod.rs
    Stage-facing orchestration and public re-exports only.

bindings.rs
    FileVisibility and HeaderImportEnvironment data shapes.

visible_names.rs
    Visible-name registry, collision checks, alias case warnings, builtin/prelude collisions.

target_resolution.rs
    Resolves parsed import paths into source or external symbol targets.

facade_resolution.rs
    Resolves source-library and module-root facade imports.

re_exports.rs
    Resolves #mod re-export clauses and updates facade export metadata.

diagnostics.rs
    Import, re-export, facade, alias, and collision diagnostic helpers.
```

## Resolution enums

Prefer explicit enums over booleans and loosely coupled maps.

```rust
pub(crate) enum ResolvedImportTarget {
    Source {
        symbol_path: InternedPath,
        export_requirement: ExportRequirement,
    },
    External {
        symbol_id: ExternalSymbolId,
    },
}
```

```rust
pub(crate) enum ExportRequirement {
    AlreadyValidatedByFacade,
    MustBeExportedFromSourceFile,
}
```

```rust
pub(crate) enum FacadeLookupResult {
    NotAFacadeImport,
    ExportedSource(InternedPath),
    ExportedExternal(ExternalSymbolId),
    NotExported {
        facade_name: String,
    },
}
```

```rust
pub(crate) enum VisibleNameBinding {
    SameFileDeclaration {
        declaration_path: InternedPath,
    },
    SourceImport {
        canonical_path: InternedPath,
    },
    TypeAlias {
        canonical_path: InternedPath,
    },
    ExternalImport {
        symbol_id: ExternalSymbolId,
    },
    Builtin,
    Prelude,
}
```

```rust
pub(crate) enum RegisterVisibleNameResult {
    Registered,
    Duplicate {
        previous: VisibleNameBinding,
    },
    CaseConventionWarning {
        warning: CompilerWarning,
    },
}
```

The exact shape can change, but resolution paths should be visible in type names and match arms.

## Constant dependency extraction

Keep constant dependency extraction outside `import_environment/`.

```text
src/compiler_frontend/headers/constant_dependencies.rs
```

Target API:

```rust
pub(crate) struct ConstantDependencyInput<'a> {
    pub(crate) headers: &'a mut [Header],
    pub(crate) module_symbols: &'a ModuleSymbols,
    pub(crate) import_environment: &'a HeaderImportEnvironment,
    pub(crate) string_table: &'a mut StringTable,
}

pub(crate) fn add_constant_initializer_dependencies(
    input: ConstantDependencyInput<'_>,
) -> Result<ConstantDependencyReport, CompilerMessages>;
```

Report shape:

```rust
pub(crate) struct ConstantDependencyReport {
    pub(crate) added_edges: usize,
    pub(crate) same_file_edges: usize,
    pub(crate) cross_file_edges: usize,
}
```

Reference classification:

```rust
pub(crate) enum ConstantReferenceResolution {
    SourceConstant {
        path: InternedPath,
        source_file: InternedPath,
    },
    SourceNonConstant {
        path: InternedPath,
    },
    SourceTypeAlias {
        path: InternedPath,
    },
    ExternalConstant {
        symbol_id: ExternalSymbolId,
    },
    ExternalNonConstant {
        symbol_id: ExternalSymbolId,
    },
    ConstructorLikeSource {
        path: InternedPath,
    },
    Unknown,
    NotVisible {
        path: InternedPath,
    },
}
```

Header should diagnose structurally clear cases:

- same-file forward constant reference
- self-reference
- visible source non-constant in constant initializer
- visible external non-constant in constant initializer, when structurally known
- not-visible constant when a same/module symbol exists but is not imported

Header should defer semantic cases to AST:

- expression type errors
- full foldability validation
- constructor argument validity
- record/choice literal semantic validation

## AST environment input

AST should receive one named input bundle.

```rust
pub(crate) struct AstEnvironmentInput {
    pub(crate) sorted_headers: Vec<Header>,
    pub(crate) module_symbols: ModuleSymbols,
    pub(crate) import_environment: HeaderImportEnvironment,
}
```

Builder shape:

```rust
pub(crate) fn build(
    mut self,
    input: AstEnvironmentInput,
    string_table: &mut StringTable,
) -> Result<AstModuleEnvironment, CompilerMessages>
```

Full AST input shape if the outer AST builder is reshaped:

```rust
pub(crate) struct AstBuildInput {
    pub(crate) headers: Vec<Header>,
    pub(crate) module_symbols: ModuleSymbols,
    pub(crate) import_environment: HeaderImportEnvironment,
    pub(crate) top_level_const_fragments: Vec<TopLevelConstFragment>,
    pub(crate) entry_runtime_fragment_count: usize,
}
```

Avoid:

```rust
pub(crate) fn build(
    headers: Vec<Header>,
    module_symbols: ModuleSymbols,
    file_visibility_by_source: FxHashMap<InternedPath, FileVisibility>,
    top_level_const_fragments: Vec<TopLevelConstFragment>,
    entry_runtime_fragment_count: usize,
    warnings: Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> ...
```

A stage contract should be a named type, not a loose list of arguments.

## ScopeContext visibility application

`ScopeContext` should consume `FileVisibility`, not import-resolution internals.

Target helper:

```rust
impl ScopeContext {
    pub(crate) fn with_file_visibility(mut self, visibility: &FileVisibility) -> ScopeContext {
        self.visible_declaration_ids = Some(visibility.visible_declaration_paths.clone());
        self.visible_source_bindings = Some(visibility.visible_source_names.clone());
        self.visible_type_aliases = Some(visibility.visible_type_alias_names.clone());
        self.visible_external_symbols = Some(visibility.visible_external_symbols.clone());
        self
    }
}
```

If cloning becomes measurable, optimize later with `Rc<FileVisibility>` or shared maps. Do not complicate the contract prematurely.

---

# Comment and style guide

## File-level docs

Every new module should start with concise WHAT / WHY / MUST NOT docs.

Example for `headers/import_environment/mod.rs`:

```rust
//! Header-stage import environment construction.
//!
//! WHAT: resolves parsed imports, re-exports, aliases, facade boundaries, and external symbols
//! into file-local visibility maps.
//! WHY: dependency sorting and AST need stable per-file visibility without rebuilding import
//! semantics in later stages.
//! MUST NOT: parse executable bodies, fold constants, or perform AST semantic validation.
```

Example for `headers/constant_dependencies.rs`:

```rust
//! Header-stage constant dependency extraction.
//!
//! WHAT: classifies symbol-shaped references captured from constant initializer tokens and adds
//! top-level dependency edges between constants.
//! WHY: dependency sorting must order constants before AST folds their initializer expressions.
//! MUST NOT: type-check expressions or decide whether a full initializer is foldable.
```

Example for `ast/module_ast/environment/constant_resolution.rs`:

```rust
//! AST constant semantic resolution.
//!
//! WHAT: parses and folds constant initializer expressions after headers have been dependency
//! sorted.
//! WHY: header sorting provides declaration order; AST still owns expression semantics.
//! MUST NOT: topologically sort constants or rebuild import visibility.
```

## Block comments

Use comments to break up complex code blocks by intent.

Good:

```rust
// Register same-file declarations before imports so aliases cannot silently shadow local names.
for declaration in same_file_declarations {
    registry.register_same_file_declaration(declaration)?;
}

// Resolve explicit imports after same-file names are known. This lets diagnostics report the
// existing visible binding instead of a generic duplicate-name error.
for import in parsed_imports {
    register_resolved_import(import, &mut registry, input)?;
}
```

Bad:

```rust
// Loop declarations.
for declaration in same_file_declarations { ... }

// Loop imports.
for import in parsed_imports { ... }
```

## Complex match comments

For large classification matches, add short comments at branch groups.

```rust
match resolution {
    // Constants create ordering edges. Same-file edges are still constrained by source order.
    ConstantReferenceResolution::SourceConstant { path, source_file } => {
        add_constant_edge(current_header, path, source_file)?;
    }

    // Type aliases live in the type namespace. They do not create value dependency edges.
    ConstantReferenceResolution::SourceTypeAlias { .. } => {}

    // These are structurally invalid in constant initializers and can fail before AST folding.
    ConstantReferenceResolution::SourceNonConstant { path }
    | ConstantReferenceResolution::ExternalNonConstant { .. } => {
        errors.push(non_constant_reference_error(path, current_header.location()));
    }

    // Unknown names may still produce a better AST diagnostic if header cannot prove visibility.
    ConstantReferenceResolution::Unknown => {}
}
```

## Function size and argument rules

- Prefer functions under ~200 lines.
- Split functions when they mix import target resolution, visible-name registration, diagnostics, and facade traversal.
- Use explicit loops for multi-stage compiler logic.
- Use iterators only for straightforward transformations.
- A function with more than five non-trivial parameters needs an input/context struct.
- No new `#[allow(clippy::too_many_arguments)]`.

---

# Phase 0 — Baseline, docs confirmation, and API contract anchoring

## Summary

Confirm the docs already state the intended stage contract, record a benchmark baseline, and add the improved plan before touching behavior.

## Implementation steps

1. Confirm these docs already include the expected contract and style guidance:

   ```text
   docs/compiler-design-overview.md
   docs/codebase-style-guide.md
   ```

2. Add this plan as:

   ```text
   docs/roadmap/plans/header_dependency_ast_contract_refactor_plan.md
   ```

   This file is intended to replace the previous plan.

3. Run baseline:

   ```bash
   just bench
   ```

4. Create or update:

   ```text
   docs/roadmap/refactors/ast-pipeline-optimisation-benchmark-log.md
   ```

   Add the baseline under:

   ```text
   Header / Dependency Sorting / AST Contract Refactor
   ```

5. Update `docs/roadmap/roadmap.md` only if this plan is not already linked.

## Audit checklist

- No code behavior changes.
- Compiler design docs and style guide already match the intended contract.
- Benchmark baseline is recorded.
- The old plan has been replaced, not supplemented by a contradictory addendum.

## Validation

Documentation-only changes do not require `just validate`, but run it if generated docs or code are touched.

---

# Phase 1 — Create header-owned import environment structure

## Summary

Create the new header-stage import environment module shape before moving behavior.

This phase establishes clean ownership and APIs first, so old AST code cannot be copied wholesale into one large file.

## Implementation steps

1. Create module directory:

   ```text
   src/compiler_frontend/headers/import_environment/
       mod.rs
       bindings.rs
       visible_names.rs
       target_resolution.rs
       facade_resolution.rs
       re_exports.rs
       diagnostics.rs
   ```

2. Add file-level docs using the WHAT / WHY / MUST NOT pattern.

3. Add `FileVisibility` and `HeaderImportEnvironment` in `bindings.rs`.

4. Add `ImportEnvironmentInput` and `prepare_import_environment(...)` in `mod.rs`.

5. Stub narrow internal functions with clear names. Avoid implementing all behavior in `mod.rs`.

   Suggested orchestration shape:

   ```rust
   pub(crate) fn prepare_import_environment(
       input: ImportEnvironmentInput<'_>,
   ) -> Result<HeaderImportEnvironment, CompilerMessages> {
       let re_exports = resolve_re_exports(ReExportInput::from(&input))?;
       let mut builder = ImportEnvironmentBuilder::new(input, re_exports);

       builder.register_same_file_declarations()?;
       builder.register_prelude_and_builtins()?;
       builder.register_file_imports()?;

       builder.finish()
   }
   ```

   This shape is illustrative. The important part is that each method has one responsibility.

6. Expose the module from `src/compiler_frontend/headers/mod.rs`.

7. Do not wire it into the pipeline yet unless doing so is small and safe.

## Tests

No new behavior tests are required if this phase only adds unused structure. Add unit tests only if `visible_names.rs` collision behavior is implemented here.

## Readability/code-shape audit

- Are the new modules narrow and documented?
- Is `mod.rs` orchestration-only?
- Are target resolution, facade resolution, visible-name registration, re-exports, and diagnostics separated?
- Are there no long argument lists?
- Are there no compatibility wrappers?
- Does every new file follow `docs/codebase-style-guide.md`?

## Validation

```bash
just bench
just validate
```

Update the benchmark log.

---

# Phase 2 — Move and rewrite import/re-export behavior into headers

## Summary

Move import binding and visibility construction from AST into the header/import-preparation layer.

This is not a file move. It is a code-quality refactor. The current AST import code is noisy, mixed-responsibility, and argument-heavy. The new header-owned implementation must split behavior into focused units.

## Implementation steps

1. Split behavior from:

   ```text
   src/compiler_frontend/ast/import_bindings.rs
   ```

   Move import-related concepts into the new header modules:

   ```text
   FileImportBindings              -> FileVisibility
   resolve_file_import_bindings    -> prepare_import_environment
   resolve_re_exports              -> import_environment/re_exports.rs
   visible-name collision helpers  -> import_environment/visible_names.rs
   import target helpers           -> import_environment/target_resolution.rs
   facade helpers                  -> import_environment/facade_resolution.rs
   import diagnostics              -> import_environment/diagnostics.rs
   ```

2. Do not copy the old file into `headers/import_environment.rs`.

3. Use `ResolvedImportTarget`, `ExportRequirement`, `FacadeLookupResult`, and `VisibleNameBinding`-style enums instead of boolean-heavy flows.

4. Replace long function signatures with input structs.

   Examples:

   ```rust
   pub(crate) struct ReExportResolutionInput<'a> {
       pub(crate) module_symbols: &'a mut ModuleSymbols,
       pub(crate) external_package_registry: &'a ExternalPackageRegistry,
       pub(crate) string_table: &'a mut StringTable,
   }
   ```

   ```rust
   pub(crate) struct ImportTargetResolutionInput<'a> {
       pub(crate) import_path: &'a InternedPath,
       pub(crate) source_file: InternedPath,
       pub(crate) module_symbols: &'a ModuleSymbols,
       pub(crate) external_package_registry: &'a ExternalPackageRegistry,
       pub(crate) string_table: &'a mut StringTable,
   }
   ```

5. Centralize diagnostics in `diagnostics.rs`.

   Suggested helpers:

   ```rust
   pub(super) fn import_name_collision(
       local_name: StringId,
       location: SourceLocation,
       previous: &VisibleNameBinding,
       string_table: &StringTable,
   ) -> CompilerError
   ```

   ```rust
   pub(super) fn not_exported_by_facade(
       import_path: &InternedPath,
       facade_name: &str,
       location: SourceLocation,
       string_table: &StringTable,
   ) -> CompilerError
   ```

   ```rust
   pub(super) fn direct_mod_file_import(
       path: &InternedPath,
       location: SourceLocation,
       string_table: &StringTable,
   ) -> CompilerError
   ```

6. Preserve behavior for:

   - source imports
   - grouped imports
   - import aliases
   - type import aliases
   - facade imports
   - source-library imports
   - `#mod.bst` re-exports
   - external package imports
   - prelude/builtin collisions
   - alias case warnings

7. Delete AST-owned import preparation once the new path is wired.

   These should be deleted or reduced to non-import AST wiring only:

   ```text
   src/compiler_frontend/ast/import_bindings.rs
   src/compiler_frontend/ast/module_ast/environment/import_environment.rs
   ```

8. Keep AST constant declaration parsing separate. If `parse_constant_header_declaration` or `ConstantHeaderParseContext` currently live in `ast/import_bindings.rs`, move them to:

   ```text
   src/compiler_frontend/ast/module_ast/environment/constant_resolution.rs
   ```

## Tests

Add or strengthen integration tests for:

- import alias collision
- prelude/builtin alias collision
- alias case warning
- grouped import per-entry alias
- grouped import group-level alias rejection
- source-library facade import success
- cross-module facade bypass rejection
- `#mod.bst` re-export success
- direct `#mod.bst` file import rejection
- external package import success
- external import alias collision
- imported type alias used by local type alias

Prefer integration tests. Add unit tests only for visible-name registry behavior that is hard to exercise end-to-end.

## Readability/code-shape audit

- Was `ast/import_bindings.rs` deleted or split?
- Was `environment/import_environment.rs` deleted or reduced to non-import wiring only?
- Did moved code avoid the old module shape?
- Are visible-name registration, facade resolution, target resolution, re-export resolution, and diagnostics independently readable?
- Are all large argument lists replaced with input/context structs?
- Are resolution outcomes represented by enums?
- Are diagnostics structured and source-located?
- Are comments helpful around non-obvious control flow?
- Does all touched code follow `docs/codebase-style-guide.md`?

## Validation

```bash
just bench
just validate
```

Update the benchmark log.

---

# Phase 3 — Thread HeaderImportEnvironment through Headers and SortedHeaders

## Summary

Make the header-built import environment part of the frontend stage contract.

AST must receive prepared visibility from header/dependency output. It must not build import visibility.

## Implementation steps

1. Extend `Headers` with:

   ```rust
   pub import_environment: HeaderImportEnvironment,
   ```

2. Wire `parse_headers` so it calls `prepare_import_environment(...)` after:

   - file headers are parsed
   - `ModuleSymbols` is built enough for import resolution
   - re-export syntax is collected

3. Extend `SortedHeaders` with:

   ```rust
   pub(crate) import_environment: HeaderImportEnvironment,
   ```

4. Thread `HeaderImportEnvironment` through dependency sorting without mutation unless dependency sorting has a specific reason to update metadata.

5. Update AST entry points to accept a named input bundle.

   Target:

   ```rust
   pub(crate) struct AstEnvironmentInput {
       pub(crate) sorted_headers: Vec<Header>,
       pub(crate) module_symbols: ModuleSymbols,
       pub(crate) import_environment: HeaderImportEnvironment,
   }
   ```

6. Update `AstModuleEnvironmentBuilder::build`.

   Remove:

   ```text
   self.resolve_import_bindings(...)
   ```

   Replace with consuming `input.import_environment`.

7. Update `ScopeContext` construction to use `FileVisibility`.

   Target helper:

   ```rust
   impl ScopeContext {
       pub(crate) fn with_file_visibility(mut self, visibility: &FileVisibility) -> ScopeContext {
           self.visible_declaration_ids = Some(visibility.visible_declaration_paths.clone());
           self.visible_source_bindings = Some(visibility.visible_source_names.clone());
           self.visible_type_aliases = Some(visibility.visible_type_alias_names.clone());
           self.visible_external_symbols = Some(visibility.visible_external_symbols.clone());
           self
       }
   }
   ```

8. Update file-level comments in AST environment builder to say AST consumes header-built visibility.

## Tests

Run all import tests added in Phase 2.

Add one integration test that proves import aliases are visible in constant initializers before AST:

```beanstalk
-- constants.bst
# site_name = "Beanstalk"

-- main.bst
import @constants/site_name as name
# title = [: [name] docs]
```

Expected: success. Dependency sorting should see the alias-resolved constant edge.

## Readability/code-shape audit

- Does `Headers` / `SortedHeaders` now carry the import environment?
- Does AST receive a named input struct rather than loose parameters?
- Does AST no longer construct file visibility?
- Is `ScopeContext` consuming `FileVisibility`, not import-resolution internals?
- Did this phase introduce any long signatures?
- Do all touched files follow `docs/codebase-style-guide.md`?

## Validation

```bash
just bench
just validate
```

Update the benchmark log.

---

# Phase 4 — Make constant initializer dependencies first-class header edges

## Summary

Use `DeclarationSyntax.initializer_references` to add real dependency edges for constants during header preparation.

Header records ordering dependencies. AST still parses and folds initializer expressions semantically.

## Implementation steps

1. Create:

   ```text
   src/compiler_frontend/headers/constant_dependencies.rs
   ```

2. Add `ConstantDependencyInput`, `ConstantDependencyReport`, and `add_constant_initializer_dependencies(...)`.

3. Call `add_constant_initializer_dependencies(...)` after import environment preparation and before dependency sorting.

4. For each `HeaderKind::Constant`, inspect `declaration.initializer_references`.

5. Resolve each reference using the current source file's `FileVisibility`.

6. Classify each reference with `ConstantReferenceResolution`.

7. Add dependency edges for cross-file source constant references.

8. Preserve same-file source-order semantics:

   - earlier same-file constant reference: valid
   - self-reference: diagnostic
   - later same-file constant reference: same-file forward-reference diagnostic
   - do not silently allow same-file forward references through sorting

9. Do not add dependency edges for:

   - external constants
   - type aliases
   - constructor-like source references
   - unknown names

10. Emit structurally clear diagnostics for:

    - source non-constant in constant initializer
    - external non-constant in constant initializer when known
    - same-file forward reference
    - self-reference
    - not-visible constant reference when a module symbol exists but is not visible

11. Leave full expression validation to AST.

12. Add detailed timers/counters if useful:

    ```text
    Header/constant initializer dependencies
    Header/constant initializer dependency edges
    ```

## Tests

Add or strengthen integration tests:

### Cross-file constant dependency sorts before AST

```beanstalk
-- a.bst
# a = 1

-- b.bst
import @a/a
# b = a + 1
```

Expected: success.

### Import alias constant dependency

```beanstalk
-- a.bst
# a = 1

-- b.bst
import @a/a as value
# b = value + 1
```

Expected: success.

### Same-file forward reference rejected

```beanstalk
# b = a + 1
# a = 1
```

Expected: clear same-file forward-reference diagnostic.

### Self-reference rejected

```beanstalk
# a = a + 1
```

Expected: clear self-reference diagnostic.

### Cross-file constant cycle rejected

```beanstalk
-- a.bst
import @b/b
# a = b + 1

-- b.bst
import @a/a
# b = a + 1
```

Expected: dependency-cycle diagnostic.

### Non-constant top-level reference rejected

```beanstalk
# make || -> Int:
    return 1
;

# value = make
```

Expected: constant initializer cannot reference non-constant value.

### Constructor-like references remain valid

Struct and choice constructors in constants should not be misclassified as invalid constant references if const records/choice construction are supported.

## Readability/code-shape audit

- Is constant dependency extraction outside `import_environment/`?
- Does it consume `HeaderImportEnvironment` instead of duplicating import lookup?
- Is reference classification represented by an enum?
- Are diagnostics extracted into helpers?
- Are same-file source-order checks readable and commented?
- Does the module avoid expression type checking and foldability logic?
- Do all touched files follow `docs/codebase-style-guide.md`?

## Validation

```bash
just bench
just validate
```

Update the benchmark log.

---

# Phase 5 — Rewrite dependency sorting around complete header-provided edges

## Summary

Dependency sorting becomes the single top-level ordering authority.

It now sorts all top-level declaration dependencies supplied by headers, including constant initializer dependencies.

## Implementation steps

1. Update module-level docs in:

   ```text
   src/compiler_frontend/module_dependencies.rs
   ```

   Required wording:

   ```text
   Dependency edges are header-provided top-level declaration dependencies.
   They include type-surface dependencies and constant initializer dependencies.
   Executable function/start body references remain excluded.
   ```

2. Remove comments claiming:

   ```text
   strict-edges-only
   initializer-expression symbols are NOT edges
   soft initializer hints
   ```

3. Audit `resolve_graph_path`.

   Because imports and paths should now be normalized earlier, dependency sorting should need less fuzzy matching.

   Do not remove fallback matching blindly. If it remains:

   - mark it as legacy tolerance
   - prefer canonical paths for all new edges
   - record a cleanup follow-up

4. Ensure `Header.dependencies` contains canonical paths where possible.

5. Keep `StartFunction` excluded from graph sorting and appended last.

6. Audit facade header handling.

   If `#mod.bst` facade headers are excluded or appended specially, document why. Do not redesign facade semantics unless required for correctness.

7. Improve dependency-cycle diagnostics if constant cycles now flow through this sorter. Diagnostics should not imply only imports are involved.

8. Ensure `ModuleSymbols.build_sorted_declarations` receives the final sorted order and no later stage mutates ordering.

## Tests

- Existing dependency sorting tests should pass.
- Add unit tests only if graph behavior is easier to inspect locally.
- Prefer integration tests from Phase 4 for user-visible behavior.

## Readability/code-shape audit

- Is there one topological sort for top-level declarations?
- Do comments match the new contract?
- Does dependency sorting avoid mentioning soft initializer hints?
- Does dependency sorting avoid compensating for AST limitations?
- Are legacy fallback path rules explicitly marked if they remain?
- Do all touched files follow `docs/codebase-style-guide.md`?

## Validation

```bash
just bench
just validate
```

Update the benchmark log.

---

# Phase 6 — Delete AST constant graph and make AST constant resolution linear

## Summary

Once constant initializer dependencies are part of header dependency sorting, `constant_graph.rs` is duplicate work and must be removed.

AST should parse and fold constants by walking sorted headers in order.

## Implementation steps

1. Delete:

   ```text
   src/compiler_frontend/ast/module_ast/environment/constant_graph.rs
   ```

2. Remove it from module declarations.

3. Remove `ordered_constant_headers` from `AstModuleEnvironmentBuilder`.

4. Move AST semantic constant parsing/folding helpers into:

   ```text
   src/compiler_frontend/ast/module_ast/environment/constant_resolution.rs
   ```

5. Update `resolve_constant_headers` to directly walk sorted headers.

   Target shape:

   ```rust
   for header in sorted_headers {
       let HeaderKind::Constant { .. } = &header.kind else {
           continue;
       };

       let declaration = parse_and_fold_constant_header(
           ConstantHeaderParseInput {
               header,
               environment: self,
               string_table,
           },
       )?;

       declaration_table.replace_by_path(declaration.clone())?;
       module_constants.push(declaration);
   }
   ```

6. Use a named input struct for constant header parsing.

   ```rust
   pub(crate) struct ConstantHeaderParseInput<'a> {
       pub(crate) header: &'a Header,
       pub(crate) environment: &'a AstModuleEnvironmentBuilder,
       pub(crate) string_table: &'a mut StringTable,
   }
   ```

   Adjust fields based on actual ownership, but avoid a long argument list.

7. Add an internal invariant diagnostic if constant parsing sees an unresolved top-level constant dependency that should have been sorted earlier.

   Do not recover by sorting again.

8. Remove AST counters/timers related to constant topological sorting.

9. Update comments:

   - constants are resolved in dependency-sorted header order
   - missing order is a header/dependency bug
   - AST still owns folding and const-only validation

## Tests

Run all constant dependency integration tests from Phase 4.

Add a regression fixture that would fail without header sorting but succeeds without the AST constant graph.

## Readability/code-shape audit

- Is `constant_graph.rs` deleted?
- Does no AST function perform topological sorting of constants?
- Is constant resolution a direct loop over sorted headers?
- Are parsing/folding APIs shaped with input structs?
- Are comments clear about header sorting vs AST folding?
- Do all touched files follow `docs/codebase-style-guide.md`?

## Validation

```bash
just bench
just validate
```

Update the benchmark log.

---

# Phase 7 — Enforce shared declaration shell ownership

## Summary

Top-level header parsing and AST body-local declaration parsing must share the same shell parser.

Header parsing stores top-level declaration shells. AST resolves those shells. AST may create body-local shells, but only through the shared `declaration_syntax` code.

## Implementation steps

1. Audit declaration parsing call sites.

   Search for duplicate logic around:

   ```text
   parse_declaration_syntax
   struct shell parsing
   choice shell parsing
   function signature parsing
   type alias parsing
   multi-bind declaration parsing
   ```

2. Clarify module ownership in `src/compiler_frontend/declaration_syntax/`.

   Required file/module doc idea:

   ```text
   declaration_syntax owns reusable declaration shell parsing for both header
   top-level declarations and AST body-local declarations. Header parsing stores
   shells; AST resolves shells.
   ```

3. Ensure top-level AST resolution consumes shell data from `HeaderKind`.

   AST should not inspect raw top-level header tokens to reconstruct:

   - constant declaration syntax
   - function signature syntax
   - struct fields
   - choice variants
   - type alias target syntax

4. Ensure body-local AST declaration parsing creates a shell first through shared declaration syntax code.

5. Rename or re-comment misleading modules if they imply ownership of full declaration parsing when they only parse shells.

6. Remove obsolete helper functions and tests that assert old duplicated behavior.

## Tests

- Existing declaration syntax tests must pass.
- Add tests only where duplicate parser removal could drift behavior.
- Prefer integration diagnostics for user-visible syntax behavior.

## Readability/code-shape audit

- Is there one shell parser?
- Do headers store top-level shells?
- Does AST resolve top-level shells instead of recreating them?
- Do body-local declarations go through the same shell parser?
- Are comments consistent about "shell" vs "resolved declaration"?
- Do all touched files follow `docs/codebase-style-guide.md`?

## Validation

```bash
just bench
just validate
```

Update the benchmark log.

---

# Phase 8 — Remove remaining AST import/order assumptions

## Summary

With import bindings and constant ordering moved out of AST, remove remaining stale assumptions from AST environment code.

## Implementation steps

1. Audit:

   ```text
   src/compiler_frontend/ast/module_ast/environment/
   src/compiler_frontend/ast/module_ast/scope_context.rs
   src/compiler_frontend/ast/mod.rs
   ```

2. Confirm AST environment builder receives:

   - sorted headers
   - `ModuleSymbols`
   - `HeaderImportEnvironment`
   - declaration shells

3. Confirm AST does not build or sort:

   - file import bindings
   - re-exports
   - constant ordering
   - top-level declaration shells

4. Update `AstModuleEnvironment` fields.

   It may store visibility for `ScopeContext`, but that data must originate from headers.

5. Update `ScopeContext` docs to describe `FileVisibility` as the source of file-local visibility.

6. Remove stale methods, fields, counters, and comments related to AST import construction and AST constant sorting.

7. Revise any remaining AST roadmap/refactor plan that still assumes old ownership.

   Required revisions:

   - remove any phase that keeps AST import binding as an AST phase
   - remove any mention of AST constant graph sorting
   - make `build_ast_environment` a linear semantic-resolution phase over sorted headers
   - keep later AST optimizations focused on context cloning, declaration table shape, expression token windows, and finalization/template churn

## Tests

No new behavior tests should be necessary unless code paths changed. Run full validation.

## Readability/code-shape audit

- Does no AST doc comment claim AST owns import binding?
- Does no AST doc comment claim AST owns constant ordering?
- Did stale fields/methods get deleted instead of left unused?
- Is `ScopeContext` visibility setup clear and documented?
- Do all touched files follow `docs/codebase-style-guide.md`?

## Validation

```bash
just bench
just validate
```

Update the benchmark log.

---

# Phase 9 — Final cleanup, documentation pass, and enforcement audit

## Summary

Remove stale comments, old assumptions, transitional code, and duplicate test fixtures.

The final code should make it hard to reintroduce an AST-side import builder or constant sorter.

## Implementation steps

1. Search for stale phrases and update/delete them:

   ```text
   strict dependency edges
   soft initializer hints
   AST import binding
   constant graph
   initializer-expression symbols are NOT edges
   AST ordering
   ordered_constant_headers
   resolve_file_import_bindings
   FileImportBindings
   ```

2. Search for code-shape risks:

   ```text
   too_many_arguments
   allow(clippy::too_many_arguments)
   resolve_re_exports
   register_source_import_binding
   register_external_import_binding
   try_resolve_facade_import
   try_resolve_module_root_facade_import
   ```

3. Update top-of-file docs in touched modules.

4. Confirm `docs/compiler-design-overview.md` and `docs/codebase-style-guide.md` still match the final implementation.

5. Update roadmap notes if any follow-up remains.

6. Prune obsolete tests that only asserted old implementation structure.

7. Run a final duplication audit.

   Confirm one owner for each responsibility:

   ```text
   imports / visibility      -> headers/import_environment/
   declaration shells        -> declaration_syntax + headers for top-level
   top-level dependency sort -> module_dependencies.rs
   constant semantic folding -> AST
   executable body parsing   -> AST
   HIR lowering              -> HIR
   borrow validation         -> borrow checker
   ```

8. Add a deferred follow-up note for header-stage parallel parsing.

   Suggested note:

   ```text
   Follow-up: evaluate Rayon-based file-level header parsing after the
   header/dependency/AST contract is stable. The follow-up must address stable
   source order, deterministic const-template IDs, StringTable access, diagnostic
   ordering, and benchmark impact.
   ```

## Tests

Run full validation and the comprehensive benchmark.

## Final readability/code-shape audit

- No AST top-level sorter remains.
- No AST import-binding builder remains.
- No duplicate declaration shell parser remains.
- Header import environment modules are narrow and documented.
- Constant dependency extraction is separate from import environment construction.
- No moved code remains in an equally noisy shape.
- No new long argument lists exist in touched code.
- No compatibility wrappers preserve old ownership.
- Comments explain WHAT/WHY/invariants rather than restating syntax.
- Benchmarks show no unexplained regression.
- Follow-up AST refactor plan is revised and ready to continue.

## Validation

```bash
just bench
just validate
```

Update the benchmark log.

---

# Expected final architecture

## Header parsing / preparation output

```rust
pub struct Headers {
    pub headers: Vec<Header>,
    pub top_level_const_fragments: Vec<TopLevelConstFragment>,
    pub entry_runtime_fragment_count: usize,
    pub module_symbols: ModuleSymbols,
    pub import_environment: HeaderImportEnvironment,
    pub warnings: Vec<CompilerWarning>,
}
```

## Dependency sorting output

```rust
pub(crate) struct SortedHeaders {
    pub(crate) headers: Vec<Header>,
    pub(crate) top_level_const_fragments: Vec<TopLevelConstFragment>,
    pub(crate) entry_runtime_fragment_count: usize,
    pub(crate) module_symbols: ModuleSymbols,
    pub(crate) import_environment: HeaderImportEnvironment,
    pub(crate) warnings: Vec<CompilerWarning>,
}
```

## AST constant resolution

```rust
for header in sorted_headers {
    let HeaderKind::Constant { .. } = &header.kind else {
        continue;
    };

    let declaration = parse_and_fold_constant_header(
        ConstantHeaderParseInput {
            header,
            environment: self,
            string_table,
        },
    )?;

    declaration_table.replace_by_path(declaration.clone())?;
    module_constants.push(declaration);
}
```

No AST graph. No retry loop. No topo-sort.

## Dependency sorting

Dependency sorting operates only on header-provided top-level dependency edges.

Those edges include:

- type-surface dependencies
- type alias target dependencies
- function signature dependencies
- struct/choice field type dependencies
- constant explicit type annotation dependencies
- constant initializer dependencies to other constants

Those edges exclude:

- executable function body references
- executable `start` body references
- body-local declarations
- AST expression implementation details

## Import visibility

Header import preparation builds:

```rust
HeaderImportEnvironment {
    file_visibility_by_source,
    warnings,
}
```

AST uses this data through `ScopeContext`.

No AST module rebuilds imports, re-exports, facades, external packages, or visible-name collisions.

## Deferred follow-up: parallel header parsing

Parallel header parsing is intentionally deferred.

A future plan may use Rayon for file-level structural header parsing after this contract is stable. That plan must handle:

- stable source-order output
- deterministic const-template IDs
- deterministic diagnostics
- `StringTable` access without broad lock contention
- import environment construction after parallel file parse
- benchmark impact on docs and speed-test builds
