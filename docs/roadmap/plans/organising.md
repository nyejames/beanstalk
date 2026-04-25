# Beanstalk Phase 0 + Phase 1 Refactor Implementation Plan
**Assumption:** Current `main` is already green, so no initial test run is required.  
**Goal:** Create a low-risk, no-behavior-change structural refactor that makes later cleanup safer.

---

## Source-of-truth files inspected

This plan is grounded in the current repo layout and these files:

- `src/compiler_frontend/headers/parse_file_headers.rs`
- `src/compiler_frontend/headers/mod.rs`
- `src/compiler_frontend/hir/hir_nodes.rs`
- `src/compiler_frontend/mod.rs`
- `src/build_system/build.rs`
- `src/compiler_frontend/datatypes.rs`
- `docs/codebase-style-guide.md`
- `docs/compiler-design-overview.md`
- `docs/roadmap/roadmap.md`

---

## Constraints

### Hard constraints

- **No semantic changes in Phase 1.**
- **No deletion of working behavior.**
- **No Phase 4 work.** Do not feature-gate the Rust interpreter, REPL, or deferred systems here.
- **No broad API redesign.** Phase 2 will handle declaration-stub cleanup. Phase 3 will handle deeper `DataType` / access separation.
- **No initial test run.** Start from the assumption that all tests are currently green.
- Keep diffs reviewable. Prefer several small commits over one large “move everything” commit.

### Style constraints

Follow `docs/codebase-style-guide.md`:

- `mod.rs` files should map module structure and expose the module surface.
- Split files by task category.
- Keep comments focused on WHAT and WHY.
- Avoid compatibility shims unless they are short-lived and deleted before the phase ends.
- Do not preserve old APIs for their own sake; Beanstalk is pre-alpha.

---

# Phase 0 — Refactor guardrails and preparation

Phase 0 should be short. It prepares the branch, creates a review checklist, and records the exact behavioral contracts that Phase 1 must preserve.

## 0.1 Create the working branch

```bash
git checkout main
git pull
git checkout -b refactor/frontend-structure-phase-0-1
```

Do **not** run the full suite here. The starting point is assumed green.

## 0.2 Confirm the current module boundaries

Use quick search/inventory commands only.

```bash
rg "parse_file_headers" src tests
rg "HeaderKind|HeaderParseOptions|TopLevelConstFragment|FileImport|Headers" src tests
rg "hir_nodes::" src tests
rg "ResolvedConstFragment" src tests
rg "\.html" src/build_system src/projects src/backends tests
```

Record the import surfaces before moving code.

Expected important consumers:

- `CompilerFrontend::tokens_to_headers` imports `HeaderParseOptions`, `Headers`, and `parse_headers`.
- `module_dependencies.rs` imports `Header`, `HeaderKind`, `Headers`, and `TopLevelConstFragment`.
- `module_symbols.rs` imports `FileImport`, `Header`, and `HeaderKind`.
- AST build code imports `Header` and `TopLevelConstFragment`.
- backends and many tests import HIR types through `crate::compiler_frontend::hir::hir_nodes::*`.

## 0.3 Decide commit boundaries

Recommended commits:

1. `refactor: split header parser types and imports`
2. `refactor: split header parser implementation modules`
3. `refactor: split HIR node model by category`
4. `refactor: rename resolved const fragment content field`
5. `docs: clarify datatype boundary comments`

Keep the `ResolvedConstFragment` rename as its own small commit. It touches generic build/backend code and is easy to review separately.

## 0.4 Write the preservation checklist

Before editing, write a short local checklist in the PR body or a scratch note.

Preserve these behaviors:

- Header parsing still produces `Headers`.
- Entry files still receive one implicit `StartFunction`.
- Non-entry top-level executable code is still rejected.
- Top-level const templates remain entry-file-only.
- Runtime template count remains authoritative in header parsing.
- `StartFunction` still has no dependency edges.
- Dependency sorting still appends `StartFunction` last.
- AST still consumes sorted headers and `ModuleSymbols`.
- HIR public type names remain unchanged after the split.
- HTML/JS/Wasm builders still consume the same HIR and fragment data.

Do not add new behavior while splitting files.

## 0.5 Identify minimum validation commands for checkpoints

Because there is no initial test run, use staged validation after changes begin.

After each commit-scale move:

```bash
cargo check
cargo fmt --check
```

After the full Phase 1 branch:

```bash
cargo test
cargo run tests
cargo clippy
cargo run --features "detailed_timers" docs
cargo run --release --features "detailed_timers" speed-test.bst
```

Or:

```bash
just validate
```

---

# Phase 1 — No-behavior structural split

Phase 1 is purely structural. It should make the code easier to navigate without changing semantics.

---

## 1.1 Split `headers/parse_file_headers.rs`

### Current problem

`parse_file_headers.rs` currently owns too many concerns:

- public header data types
- parse options
- per-file parsing state
- per-header build state
- `parse_headers`
- `build_module_symbols`
- per-file token classification
- import parsing/normalization
- top-level declaration dispatch
- implicit `start` capture
- top-level const template placement
- duplicate declaration detection
- strict dependency edge collection
- function body token capture
- deferred/reserved syntax checks

The top-level comments are useful, but the file is acting as a module, not a focused implementation file.

### Target layout

Create:

```text
src/compiler_frontend/headers/
├── mod.rs
├── types.rs
├── parse_file_headers.rs
├── file_parser.rs
├── header_dispatch.rs
├── imports.rs
├── const_fragments.rs
├── start_capture.rs
├── dependency_edges.rs
└── module_symbols.rs
```

`module_symbols.rs` already exists. Do not refactor its internals in Phase 1 beyond import path updates.

### 1.1.1 Update `headers/mod.rs`

Current:

```rust
pub(crate) mod module_symbols;
pub(crate) mod parse_file_headers;
```

Target:

```rust
//! Header parsing stage modules.
//!
//! WHAT: extracts file-level declarations/imports and start-function boundaries before AST build.
//! Header parsing also owns top-level symbol collection (`module_symbols`), so dependency sorting
//! and AST construction receive a pre-built symbol package without a separate manifest stage.

mod const_fragments;
mod dependency_edges;
mod file_parser;
mod header_dispatch;
mod imports;
pub(crate) mod module_symbols;
pub(crate) mod parse_file_headers;
mod start_capture;
mod types;

pub(crate) use parse_file_headers::parse_headers;
pub(crate) use types::{
    FileImport, Header, HeaderKind, HeaderParseOptions, Headers, TopLevelConstFragment,
};
```

If keeping existing imports stable is easier for the first commit, temporarily also re-export from `parse_file_headers.rs`, then delete those re-exports before the phase ends.

Preferred final state: consumers import from `crate::compiler_frontend::headers::{...}` where practical.

---

## 1.2 Move header data types into `headers/types.rs`

Move these from `parse_file_headers.rs`:

- `Headers`
- `TopLevelConstFragment`
- `HeaderParseOptions`
- `HeaderKind`
- `Header`
- `impl Display for Header`
- `impl Header`
- `FileImport`

`types.rs` should own only data shapes and simple impls.

Suggested file header:

```rust
//! Header-stage data contracts.
//!
//! WHAT: shared structs/enums produced by header parsing and consumed by dependency sorting,
//! AST construction, and module symbol collection.
//! WHY: keeping these types separate from parser control flow makes the header-stage API obvious
//! and avoids making `parse_file_headers.rs` the dumping ground for every header concern.
```

### Required import updates

Change imports such as:

```rust
use crate::compiler_frontend::headers::parse_file_headers::{
    Header, HeaderKind, Headers, TopLevelConstFragment,
};
```

to either:

```rust
use crate::compiler_frontend::headers::{
    Header, HeaderKind, Headers, TopLevelConstFragment,
};
```

or:

```rust
use crate::compiler_frontend::headers::types::{
    Header, HeaderKind, Headers, TopLevelConstFragment,
};
```

Prefer the first shape if `headers/mod.rs` re-exports the stage API.

### Checkpoint

```bash
cargo check
cargo fmt --check
```

---

## 1.3 Keep `parse_file_headers.rs` as orchestration only

After moving types, reduce `parse_file_headers.rs` to:

- `parse_headers`
- `build_module_symbols`
- setup of `HeaderParseContext`
- final assembly of `Headers`

Move private parsing internals out.

The final purpose of this file:

```rust
//! Header parser entry point.
//!
//! WHAT: orchestrates parsing all tokenized files into `Headers`, gathers top-level const-fragment
//! placement metadata, and builds the header-owned `ModuleSymbols` package.
//! WHY: callers should have one obvious entry function while detailed file/header parsing lives in
//! focused helper modules.
```

### Keep here

- `parse_headers`
- `build_module_symbols`

### Move out

- `parse_headers_in_file`
- `starts_duplicate_top_level_header_declaration`
- `normalize_import_dependency_path`
- `create_header`
- `create_top_level_const_template`
- runtime template push helpers
- strict edge helpers
- function-body capture helpers

---

## 1.4 Create `headers/file_parser.rs`

Move:

- `parse_headers_in_file`
- `HeaderParseContext`
- `HeaderBuildContext` if it remains shared by file parsing and dispatch
- top-level symbol classification logic
- `starts_duplicate_top_level_header_declaration`

Suggested file header:

```rust
//! Per-file header splitting.
//!
//! WHAT: walks one tokenized Beanstalk file and separates top-level declarations from the implicit
//! entry `start` body.
//! WHY: file-level control flow is different from declaration-specific parsing; keeping it separate
//! prevents the header entry point from becoming a parser monolith.
```

### Boundary rules

`file_parser.rs` should call into:

- `imports::parse_file_import`
- `header_dispatch::create_header`
- `const_fragments::create_top_level_const_template`
- `start_capture::push_runtime_template_tokens_to_start_function`

It should not own:

- declaration shell parsing details
- import normalization details
- const-template body capture details
- dependency edge collection internals

### Public visibility

Most functions should be:

```rust
pub(super)
```

Use `pub(crate)` only if used outside `headers/`.

---

## 1.5 Create `headers/imports.rs`

Move:

- `normalize_import_dependency_path`
- import clause handling helper from the `TokenKind::Import` arm if extracted

Suggested helper shape:

```rust
pub(super) fn collect_file_imports_from_clause(
    token_stream: &mut FileTokens,
    current_location: SourceLocation,
    source_file: &InternedPath,
    file_import_paths: &mut HashSet<InternedPath>,
    file_imports: &mut Vec<FileImport>,
    encountered_symbols: &mut HashSet<StringId>,
    string_table: &mut StringTable,
) -> Result<(), CompilerError>
```

This keeps the `TokenKind::Import` branch in `file_parser.rs` small.

Suggested file header:

```rust
//! Header-stage import collection.
//!
//! WHAT: parses top-level import clauses into normalized header dependency paths.
//! WHY: imports affect file-local visibility and strict dependency edges, so their normalization
//! belongs to the header stage rather than AST body parsing.
```

### Important preservation

Keep relative import behavior unchanged:

- `@./x`
- `@..`
- non-relative imports passed through unchanged

---

## 1.6 Create `headers/header_dispatch.rs`

Move:

- `create_header`
- kind-specific declaration dispatch helpers
- function header parsing glue
- struct header parsing glue
- choice header parsing glue
- exported constant header parsing glue
- trait/reserved syntax checks used during declaration dispatch
- `capture_function_body_tokens` if not placed separately

Suggested file header:

```rust
//! Header declaration dispatch.
//!
//! WHAT: classifies one top-level declaration after its leading symbol has been seen and builds the
//! concrete `HeaderKind` payload.
//! WHY: declaration-kind parsing is separate from per-file token walking and from dependency sorting.
```

### Boundary

This module may call:

- `dependency_edges::collect_named_type_dependency_edge`
- declaration syntax parsers
- body token capture helpers

It should not:

- decide whether a token starts a top-level statement
- mutate file-level import sets
- assemble final `Headers`

---

## 1.7 Create `headers/dependency_edges.rs`

Move:

- `collect_named_type_dependency_edge`
- any helper that decides whether a named type reference becomes a strict dependency edge

Suggested file header:

```rust
//! Strict header dependency edge collection.
//!
//! WHAT: converts type references from declaration shells into dependency edges for top-level
//! declaration sorting.
//! WHY: dependency sorting uses strict structural edges only; expression/body references stay soft
//! and are resolved later by AST.
```

### Important preservation

Do not start collecting constant initializer references here. The current contract is strict edges only:

- function signature type refs
- struct field type refs
- constant declared type refs

Do **not** include:

- function body references
- constant initializer expression symbols
- entry `start` body references
- top-level runtime template captures

---

## 1.8 Create `headers/const_fragments.rs`

Move:

- `create_top_level_const_template`
- `TOP_LEVEL_CONST_TEMPLATE_NAME` handling
- const template numbering
- compile-time fragment header creation helpers

Suggested file header:

```rust
//! Top-level const-template header creation.
//!
//! WHAT: turns entry-file `#[...]` templates into const-template headers plus placement metadata.
//! WHY: const fragments are folded by AST but ordered by header parsing through runtime insertion
//! indices, so this logic must stay in the header stage.
```

### Important preservation

Header parsing remains the authoritative owner of:

- `runtime_insertion_index`
- `entry_runtime_fragment_count`
- entry-file-only const templates

Do not move fragment placement to AST or HIR.

---

## 1.9 Create `headers/start_capture.rs`

Move:

- `push_runtime_template_tokens_to_start_function`
- any balanced-template capture helper specifically for pushing runtime template tokens into `start`
- non-entry executable check helper, if extracted

Suggested file header:

```rust
//! Implicit entry-start body capture.
//!
//! WHAT: collects non-header top-level tokens into the module entry file's implicit `start` body.
//! WHY: only the entry file executes top-level runtime code; non-entry executable code must be
//! rejected before AST lowering.
```

### Important preservation

Keep the current rule:

- entry file top-level runtime code becomes `StartFunction`
- non-entry top-level executable code is rejected
- `StartFunction` has no dependency edges
- `StartFunction` is appended last by dependency sorting

---

## 1.10 After header split, update affected imports

Likely affected files:

- `src/compiler_frontend/pipeline.rs`
- `src/compiler_frontend/module_dependencies.rs`
- `src/compiler_frontend/headers/module_symbols.rs`
- `src/compiler_frontend/ast/mod.rs`
- `src/compiler_frontend/ast/module_ast/build_state.rs`
- tests under `src/compiler_frontend/headers/tests/`
- any frontend test support that imports `parse_file_headers::*`

Use:

```bash
rg "headers::parse_file_headers" src tests
rg "parse_file_headers::" src tests
```

Target: only `parse_headers` should still come from `parse_file_headers`, or even that should be re-exported through `headers`.

### Checkpoint

```bash
cargo check
cargo fmt --check
```

---

# 1.11 Split `hir/hir_nodes.rs`

## Current problem

`hir_nodes.rs` currently contains all major HIR concepts in one file:

- IDs
- module
- constants
- doc fragments
- regions
- structs
- functions
- blocks
- locals
- places
- statements
- terminators
- expressions
- patterns
- operators

It has good comments, but it is becoming a broad data-model file.

## Target layout

Create:

```text
src/compiler_frontend/hir/
├── ids.rs
├── module.rs
├── constants.rs
├── regions.rs
├── functions.rs
├── blocks.rs
├── places.rs
├── statements.rs
├── terminators.rs
├── expressions.rs
├── patterns.rs
├── operators.rs
└── hir_nodes.rs
```

Keep `hir_nodes.rs` as a compatibility re-export module during Phase 1:

```rust
//! HIR data-model re-export surface.
//!
//! WHAT: keeps the existing `hir::hir_nodes::*` import path stable while the HIR data model is
//! split into focused files.
//! WHY: Phase 1 is structural only. Existing backends/tests should not need semantic rewrites.

pub use super::blocks::*;
pub use super::constants::*;
pub use super::expressions::*;
pub use super::functions::*;
pub use super::ids::*;
pub use super::module::*;
pub use super::operators::*;
pub use super::patterns::*;
pub use super::places::*;
pub use super::regions::*;
pub use super::statements::*;
pub use super::terminators::*;
```

This intentionally preserves existing import paths such as:

```rust
use crate::compiler_frontend::hir::hir_nodes::HirModule;
```

### Update `hir/mod.rs`

Add the new modules:

```rust
pub(crate) mod blocks;
pub(crate) mod constants;
pub(crate) mod expressions;
pub(crate) mod functions;
pub(crate) mod ids;
pub(crate) mod module;
pub(crate) mod operators;
pub(crate) mod patterns;
pub(crate) mod places;
pub(crate) mod regions;
pub(crate) mod statements;
pub(crate) mod terminators;

pub(crate) mod hir_nodes;
```

Preserve existing module visibility until the compiler builds. Tighten later only if easy.

---

## 1.12 Move HIR IDs into `hir/ids.rs`

Move:

- `define_hir_id!`
- `HirNodeId`
- `HirValueId`
- `BlockId`
- `LocalId`
- `StructId`
- `FieldId`
- `FunctionId`
- `RegionId`
- `HirConstId`
- `ChoiceId`

Suggested file header:

```rust
//! Stable HIR ID newtypes.
//!
//! WHAT: dense IDs used to index HIR modules, blocks, locals, expressions, constants, and choices.
//! WHY: HIR facts and side tables refer to semantic objects by ID rather than by AST paths.
```

---

## 1.13 Move constants/doc fragments into `hir/constants.rs`

Move:

- `HirDocFragmentKind`
- `HirDocFragment`
- `HirConstField`
- `HirConstValue`
- `HirModuleConst`
- `ResultVariant` only if it is used primarily by constants/results

Decision point:

- If `ResultVariant` is used across expressions and constants, put it in `expressions.rs` or a small `variants.rs`.
- Simpler Phase 1 choice: keep `ResultVariant` in `expressions.rs` and import it into `constants.rs`.

Suggested file header:

```rust
//! HIR compile-time constants and documentation fragments.
//!
//! WHAT: data carried from AST into HIR for module constants and extracted documentation output.
//! WHY: constants are backend/tooling metadata, not ordinary runtime statements.
```

---

## 1.14 Move regions into `hir/regions.rs`

Move:

- `HirRegion`

Suggested file header:

```rust
//! HIR lexical regions.
//!
//! WHAT: region nodes used by HIR locals, blocks, and later lifetime/ownership analysis.
//! WHY: regions give borrow validation and future lowering passes a stable scope tree.
```

---

## 1.15 Move functions into `hir/functions.rs`

Move:

- `HirFunctionOrigin`
- `HirFunction`

Suggested file header:

```rust
//! HIR function declarations.
//!
//! WHAT: function-level HIR metadata, including entry block, parameters, return type, and semantic
//! origin classification.
//! WHY: backends need to distinguish regular functions from the implicit entry `start` function.
```

---

## 1.16 Move blocks/locals into `hir/blocks.rs`

Move:

- `HirBlock`
- `HirLocal`

Suggested file header:

```rust
//! HIR blocks and locals.
//!
//! WHAT: explicit control-flow blocks plus locals declared inside those blocks.
//! WHY: borrow checking, backend lowering, and diagnostics all operate over block/local IDs.
```

---

## 1.17 Move places into `hir/places.rs`

Move:

- `HirPlace`

Suggested file header:

```rust
//! HIR memory places.
//!
//! WHAT: canonical memory projections such as locals, fields, and indexed elements.
//! WHY: assignments, loads, copies, and borrow checking need one shared place representation.
```

---

## 1.18 Move statements into `hir/statements.rs`

Move:

- `HirStatement`
- `HirStatementKind`

Suggested file header:

```rust
//! HIR statements.
//!
//! WHAT: effectful operations inside HIR blocks.
//! WHY: statements are where assignment, calls, side-effect expressions, and runtime fragment pushes
//! become explicit before borrow validation and backend lowering.
```

Import dependencies:

- `HirExpression`
- `HirNodeId`
- `LocalId`
- `HirPlace`
- `CallTarget`
- `SourceLocation`

---

## 1.19 Move terminators into `hir/terminators.rs`

Move:

- `HirTerminator`

Suggested file header:

```rust
//! HIR block terminators.
//!
//! WHAT: explicit control-flow exits for each block.
//! WHY: control flow must be structured enough for borrow validation and backend lowering.
```

Import dependencies:

- `HirExpression`
- `HirMatchArm`
- `BlockId`
- `LocalId`

---

## 1.20 Move expressions into `hir/expressions.rs`

Move:

- `HirExpression`
- `ValueKind`
- `HirExpressionKind`
- `OptionVariant`
- `HirBuiltinCastKind`
- `ResultVariant` if not split elsewhere

Suggested file header:

```rust
//! HIR expressions.
//!
//! WHAT: typed value-producing nodes used by statements, terminators, and pattern matching.
//! WHY: HIR keeps normal value construction as expression trees while control flow stays explicit.
```

Import dependencies:

- IDs
- places
- operators
- type IDs
- `StringId`

---

## 1.21 Move patterns into `hir/patterns.rs`

Move:

- `HirMatchArm`
- `HirRelationalPatternOp`
- `HirPattern`

Suggested file header:

```rust
//! HIR pattern matching data.
//!
//! WHAT: lowered pattern arms for HIR match terminators.
//! WHY: AST validates patterns and exhaustiveness; HIR preserves the validated matching contract for
//! backend lowering.
```

---

## 1.22 Move operators into `hir/operators.rs`

Move:

- `HirBinOp`
- `HirUnaryOp`

Suggested file header:

```rust
//! HIR operators.
//!
//! WHAT: normalized binary and unary operator enums used by HIR expressions.
//! WHY: backends should consume semantic operators rather than frontend token kinds.
```

---

## 1.23 Move module struct into `hir/module.rs`

Move:

- `HirChoice`
- `HirChoiceVariant`
- `HirModule`
- `impl HirModule`

Suggested file header:

```rust
//! HIR module container.
//!
//! WHAT: the complete semantic IR payload produced for one Beanstalk module.
//! WHY: backends consume `HirModule` as the stable frontend output after AST lowering and borrow
//! validation.
```

Import dependencies:

- all moved HIR types
- `TypeContext`
- `HirSideTable`
- `CompilerWarning`
- `RenderedPathUsage`
- `StringIdRemap`
- `FxHashMap`

### Checkpoint

```bash
cargo check
cargo fmt --check
```

---

# 1.24 Rename `ResolvedConstFragment.html`

## Current problem

`ResolvedConstFragment` lives in generic build-system code, but its field is named `html`.

Current shape:

```rust
pub struct ResolvedConstFragment {
    pub runtime_insertion_index: usize,
    pub html: String,
}
```

This is HTML-specific naming in a generic build contract.

## Target

Rename to:

```rust
pub struct ResolvedConstFragment {
    pub runtime_insertion_index: usize,
    pub rendered_text: String,
}
```

Recommended name: `rendered_text`.

Why not `content`? `rendered_text` is more explicit and still backend-neutral.

### Update comments

Before:

```rust
/// The rendered HTML/text content of this const fragment.
pub html: String,
```

After:

```rust
/// The rendered text content of this const fragment.
pub rendered_text: String,
```

Update nearby comment in `Module::remap_string_ids`:

```rust
// ResolvedConstFragment.rendered_text is already a String.
```

### Update consumers

Search:

```bash
rg "\.html" src/build_system src/projects src/backends tests
rg "ResolvedConstFragment" src tests
```

Likely places:

- `src/build_system/create_project_modules/frontend_orchestration.rs`
- `src/projects/html_project/js_path.rs`
- `src/projects/html_project/wasm/artifacts.rs`
- HTML project tests

Preserve HTML builder names where they genuinely mean emitted HTML. Only rename the generic fragment field.

### Checkpoint

```bash
cargo check
cargo fmt --check
```

---

# 1.25 Add a module doc comment to `datatypes.rs`

Do **not** refactor `DataType` in Phase 1. That belongs to Phase 3.

Add a clear module-level comment at the top:

```rust
//! Frontend semantic data types.
//!
//! WHAT: `DataType` represents the frontend's current semantic type shapes during header parsing,
//! AST construction, and compatibility checking.
//!
//! WHY: this module is still the bridge between early frontend syntax and later HIR `TypeId`
//! lowering. Some variants currently carry ownership/access information because the pre-alpha
//! frontend has not yet fully separated semantic type identity from binding/access state.
//!
//! Phase note:
//! - Phase 1 only documents this boundary.
//! - A later refactor should separate pure type identity from mutability/access/ownership facts.
```

This prevents the current design from looking accidental while avoiding a semantic rewrite.

### Checkpoint

```bash
cargo check
cargo fmt --check
```

---

# 1.26 Final Phase 1 validation

After all Phase 1 commits:

```bash
cargo fmt --check
cargo check
cargo test
cargo run tests
cargo clippy
cargo run --features "detailed_timers" docs
cargo run --release --features "detailed_timers" speed-test.bst
```

Or:

```bash
just validate
```

Expected result: no behavior change.

---

# Phase 1 review checklist

## Header split review

- [ ] `parse_file_headers.rs` is now orchestration only.
- [ ] `headers/types.rs` owns header data contracts.
- [ ] `file_parser.rs` owns one-file token splitting.
- [ ] `header_dispatch.rs` owns declaration-kind classification.
- [ ] `imports.rs` owns import normalization/collection.
- [ ] `const_fragments.rs` owns top-level const-template header creation.
- [ ] `start_capture.rs` owns implicit start token capture helpers.
- [ ] `dependency_edges.rs` owns strict dependency edge collection.
- [ ] No behavior changes to `StartFunction`.
- [ ] No new dependency edges from expression bodies or constant initializers.
- [ ] No AST top-level symbol rediscovery introduced.

## HIR split review

- [ ] Existing `hir::hir_nodes::*` imports still work.
- [ ] New files each own one concept.
- [ ] `hir_nodes.rs` is only a re-export surface.
- [ ] No HIR field/type names changed.
- [ ] No backend/HIR lowering semantics changed.

## Build fragment rename review

- [ ] Generic build-system field is no longer named `html`.
- [ ] HTML-specific names remain where they actually mean emitted HTML.
- [ ] `ResolvedConstFragment.rendered_text` is used consistently.

## Comments review

- [ ] New files have module-level WHAT/WHY comments.
- [ ] Comments explain stage boundaries, not syntax.
- [ ] `datatypes.rs` explicitly documents the current boundary and future cleanup.

---

# Risks and mitigations

## Risk: import churn causes hidden breakage

Mitigation:

- Keep `hir_nodes.rs` as a re-export surface.
- Re-export header public types from `headers/mod.rs`.
- Use `cargo check` after each split.

## Risk: moving header helpers accidentally changes visibility

Mitigation:

- Start with `pub(super)` for cross-header-module helpers.
- Use `pub(crate)` only when required by non-header modules.
- Avoid private helper renames during the first move.

## Risk: const/runtime fragment ordering changes

Mitigation:

- Do not change runtime fragment counting logic.
- Move code mechanically first.
- Keep `entry_runtime_fragment_count` owned by header parsing.
- Validate with existing const/runtime interleave tests.

## Risk: HIR re-export masks circular imports

Mitigation:

- Split in dependency order:
  1. IDs
  2. operators
  3. places/expressions
  4. patterns
  5. statements/terminators
  6. blocks/functions/regions/constants/module
- If cycles appear, move shared tiny enums into the lower-level module that has fewer dependencies.

## Risk: rename `.html` field catches unrelated HTML output names

Mitigation:

- Rename only `ResolvedConstFragment.html`.
- Do not rename `HtmlProjectBuilder`, `html_output_path`, `FileKind::Html`, or generated HTML artifact names.

---

# Explicit non-goals

Do not do these in Phase 0 or Phase 1:

- Do not remove `declaration_stubs_by_path`.
- Do not change `ModuleSymbols` internals beyond imports.
- Do not alter dependency sorting behavior.
- Do not remove start-function matching logic yet.
- Do not split `DataType` into type/access layers.
- Do not feature-gate Rust interpreter or REPL.
- Do not change HIR semantics.
- Do not rewrite tests unless imports break.

---

# Recommended final commit sequence

```text
1. refactor(headers): move header data contracts into types module
2. refactor(headers): split file parsing and import helpers
3. refactor(headers): split dispatch, const fragment, start capture, and dependency edge helpers
4. refactor(hir): split HIR data model into focused modules
5. refactor(build): rename resolved const fragment html field
6. docs(frontend): clarify datatype ownership/access boundary
```

This gives clean review points and makes revert/debug easier.
