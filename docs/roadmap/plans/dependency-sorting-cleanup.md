# Beanstalk Phase 2 Implementation Plan

## Title

**Phase 2: Make dependency sorting the single producer of AST declaration inputs**

## Purpose

Phase 2 removes the transitional duplication between:

- `ModuleSymbols.declarations`
- `ModuleSymbols.declaration_stubs_by_path`
- AST-side fallback declaration seeding in `AstBuildState::new`

The outcome should be a stricter compiler contract:

> Header parsing discovers top-level symbols and declaration shells. Dependency sorting builds the complete sorted declaration placeholder list. AST consumes that list directly and does not reconstruct, backfill, or re-discover top-level declaration stubs.

This phase is intentionally narrower than the full type/access cleanup. It does **not** remove ownership from `DataType`, does **not** redesign constant resolution, and does **not** change language behavior.

---

## Current repo-grounded state

### Relevant files

| File | Current role |
|---|---|
| `src/compiler_frontend/headers/module_symbols.rs` | Owns `ModuleSymbols`, declaration stub construction, builtin staging, symbol maps |
| `src/compiler_frontend/module_dependencies.rs` | Sorts top-level headers, appends start headers, calls `module_symbols.build_sorted_declarations` |
| `src/compiler_frontend/ast/module_ast/build_state.rs` | Takes `module_symbols.declarations`, then backfills missing declarations from `declaration_stubs_by_path` |
| `src/compiler_frontend/ast/module_ast/pass_type_resolution.rs` | Resolves constants and structs; uses `declaration_stubs_by_path` to detect deferrable constant-resolution errors |
| `src/compiler_frontend/ast/import_bindings.rs` | Builds file-local import visibility from `ModuleSymbols` maps |
| `src/compiler_frontend/ast/module_ast/scope_context.rs` | Builds `TopLevelDeclarationIndex` from the AST declaration vector |
| `src/compiler_frontend/headers/parse_file_headers.rs` | Calls `build_module_symbols`, which currently calls `seed_declaration_stubs` |
| `src/compiler_frontend/ast/mod.rs` | AST entry point; receives `ModuleSymbols` and sorted headers |

---

## Problem statement

`ModuleSymbols` currently has two overlapping declaration representations:

```rust
pub(crate) declarations: Vec<Declaration>,
pub(crate) declaration_stubs_by_path: FxHashMap<InternedPath, DeclarationStub>,
```

`declarations` is filled in dependency sorting:

```rust
module_symbols.build_sorted_declarations(&sorted, string_table);
```

But `AstBuildState::new` then does this:

```rust
let mut declarations = std::mem::take(&mut module_symbols.declarations);

for stub in module_symbols.declaration_stubs_by_path.values() {
    if declarations
        .iter()
        .any(|declaration| declaration.id == stub.path)
    {
        continue;
    }

    declarations.push(stub.declaration.to_owned());
}
```

This fallback means AST still depends on a second declaration-stub source.

The important repo-specific detail is that this fallback is **not purely redundant today**. It currently backfills declaration kinds that `build_sorted_declarations` does not include.

Current `build_sorted_declarations` only pushes stubs matching:

```rust
DeclarationStubKind::Function
| DeclarationStubKind::Choice
| DeclarationStubKind::StartFunction
```

So `Constant` and `Struct` placeholders are currently entering AST through `declaration_stubs_by_path`, not through the sorted declaration list.

That means the safe cleanup order is:

1. Make `build_sorted_declarations` produce a complete declaration placeholder list.
2. Adjust constant deferral logic so it does not require `DeclarationStub`.
3. Remove `declaration_stubs_by_path`.
4. Remove AST fallback seeding.
5. Validate no stage reconstructs top-level declaration stubs.

---

## Desired final contract

After Phase 2:

### `ModuleSymbols`

`ModuleSymbols` should hold:

- sorted declaration placeholders in `declarations`
- staged builtin declarations in `builtin_declarations` until dependency sorting consumes them
- order-independent symbol/import/export/source maps
- builtin struct AST/type metadata

It should **not** hold a separate `declaration_stubs_by_path`.

Recommended final shape:

```rust
#[derive(Debug)]
pub(crate) struct ModuleSymbols {
    // Complete top-level declaration placeholders in sorted-header order.
    // Empty after parse_headers; filled by resolve_module_dependencies.
    pub(crate) declarations: Vec<Declaration>,

    // Staged during header parsing; appended by resolve_module_dependencies.
    pub(crate) builtin_declarations: Vec<Declaration>,

    // Order-independent maps built during header parsing.
    pub(crate) canonical_source_by_symbol_path: FxHashMap<InternedPath, InternedPath>,
    pub(crate) module_file_paths: FxHashSet<InternedPath>,
    pub(crate) file_imports_by_source: FxHashMap<InternedPath, Vec<FileImport>>,
    pub(crate) importable_symbol_exported: FxHashMap<InternedPath, bool>,
    pub(crate) declared_paths_by_file: FxHashMap<InternedPath, FxHashSet<InternedPath>>,
    pub(crate) declared_names_by_file: FxHashMap<InternedPath, FxHashSet<StringId>>,

    // Builtin data merged during header parsing.
    pub(crate) builtin_visible_symbol_paths: FxHashSet<InternedPath>,
    pub(crate) builtin_struct_ast_nodes: Vec<AstNode>,
    pub(crate) resolved_struct_fields_by_path: FxHashMap<InternedPath, Vec<Declaration>>,
    pub(crate) struct_source_by_path: FxHashMap<InternedPath, InternedPath>,
}
```

### `AstBuildState::new`

`AstBuildState::new` should only do:

```rust
let declarations = std::mem::take(&mut module_symbols.declarations);
```

It should not inspect headers, declaration stubs, or fallback maps.

### Constant deferral

`pass_type_resolution.rs` should not import or inspect `DeclarationStub` / `DeclarationStubKind`.

Instead, constant deferral should use a set of constant header paths derived from `sorted_headers`:

```rust
let constant_header_paths = sorted_headers
    .iter()
    .filter(|header| matches!(header.kind, HeaderKind::Constant { .. }))
    .map(|header| header.tokens.src_path.to_owned())
    .collect::<FxHashSet<_>>();
```

Then:

```rust
fn is_deferrable_constant_resolution_error(
    error: &CompilerError,
    visible_symbol_paths: &FxHashSet<InternedPath>,
    constant_header_paths: &FxHashSet<InternedPath>,
    string_table: &mut StringTable,
) -> bool
```

This preserves the current behavior without needing a full declaration stub map.

---

## Non-goals

Do **not** include these in Phase 2:

- Removing ownership from `DataType`
- Replacing constant placeholders instead of appending resolved constants, unless done as a separate carefully tested follow-up
- Changing import semantics
- Changing header sorting semantics
- Changing bare-file import behavior
- Changing builtin registration semantics
- Reworking receiver-method catalog construction
- Reworking template fragment handling
- Feature-gating deferred systems
- Splitting files further beyond what Phase 1 already did

This phase should mostly be a data-flow cleanup.

---

# Implementation plan

## Step 1 — Add guard tests before deleting the fallback

The repo is currently green, so do **not** start with a full validation run. Start by adding focused regression tests that will fail if sorted declaration placeholders become incomplete.

These tests should be added before the code cleanup, because they encode the current required behavior.

### 1.1 Add a constant-to-constant dependency case

Add an integration fixture under:

```text
tests/cases/constant_soft_dependency_resolves_after_placeholder/input/#page.bst
tests/cases/constant_soft_dependency_resolves_after_placeholder/expect.toml
```

Example source shape:

```beanstalk
#second Int = first + 2
#first Int = 40

[:[second]]
```

Expected:

- success
- rendered output contains `42`
- warnings forbid

Why this matters:

- constant initializer dependencies are soft, not strict dependency edges
- the first constant must see the second constant placeholder/deferred resolution machinery
- removing `declaration_stubs_by_path` must not break this

Add to `tests/cases/manifest.toml`:

```toml
[[case]]
id = "constant_soft_dependency_resolves_after_placeholder"
path = "constant_soft_dependency_resolves_after_placeholder"
tags = ["integration", "constants", "dependency-resolution"]
```

### 1.2 Add a cross-file imported constant dependency case

Add:

```text
tests/cases/constant_cross_file_soft_dependency/input/#page.bst
tests/cases/constant_cross_file_soft_dependency/input/values.bst
tests/cases/constant_cross_file_soft_dependency/expect.toml
```

Example:

```beanstalk
-- #page.bst
@values/later
@values/base

#total Int = later + 1

[:[total]]
```

```beanstalk
-- values.bst
#later Int = base + 10
#base Int = 31
```

Expected output contains `42`.

Why this matters:

- tests imported constant placeholders
- tests file-local visibility gates
- tests deferred resolution without relying on declaration stubs

### 1.3 Add a struct type placeholder case

Add:

```text
tests/cases/struct_placeholder_visible_before_resolution/input/#page.bst
tests/cases/struct_placeholder_visible_before_resolution/expect.toml
```

Example shape:

```beanstalk
make_user || -> User:
    User(name = "Nye")
;

User = |
    name String
|

user = make_user()
[:[user.name]]
```

Expected output contains `Nye`.

Why this matters:

- current AST fallback backfills `Struct` placeholder stubs
- after Phase 2, sorted declarations must include struct placeholders without AST fallback

Adjust syntax to match current accepted struct constructor syntax in existing fixtures if needed.

### 1.4 Add same-name / visibility collision guard if not already covered

Before changing declaration seeding, confirm these existing cases still cover the behavior:

- duplicate function names
- duplicate struct names
- duplicate function/struct names
- import visibility non-exported
- bare-file import rejected

If existing integration cases already cover these, do not add duplicates.

---

## Step 2 — Make sorted declarations complete

Edit:

```text
src/compiler_frontend/headers/module_symbols.rs
```

### Current behavior

`build_sorted_declarations` skips constants and structs:

```rust
if let Some(stub) = declaration_stub_from_header(header, string_table)
    && matches!(
        stub.kind,
        DeclarationStubKind::Function
            | DeclarationStubKind::Choice
            | DeclarationStubKind::StartFunction
    )
{
    self.declarations.push(stub.declaration);
}
```

### Change

Make it push every declaration stub returned by `declaration_stub_from_header`.

Recommended implementation:

```rust
pub(crate) fn build_sorted_declarations(
    &mut self,
    sorted_headers: &[Header],
    string_table: &mut StringTable,
) {
    self.declarations.clear();

    for header in sorted_headers {
        if let Some(stub) = declaration_stub_from_header(header, string_table) {
            self.declarations.push(stub.declaration);
        }
    }

    self.declarations.append(&mut self.builtin_declarations);
}
```

### Important notes

1. Keep `HeaderKind::ConstTemplate => None`.
2. Keep `StartFunction` included.
3. Keep builtins appended at the end for now, preserving current behavior.
4. This step intentionally means constants and structs enter `declarations` through the sorted declaration list, not the fallback map.

### Review target

After this step, `module_symbols.declarations` must contain placeholders for:

- functions
- constants
- structs
- choices
- start function
- builtin declarations

---

## Step 3 — Temporarily keep the fallback but prove it is inactive

Before deleting `declaration_stubs_by_path`, keep the fallback in `AstBuildState::new` for one intermediate commit.

Add a debug-only assertion to prove it no longer contributes anything.

Edit:

```text
src/compiler_frontend/ast/module_ast/build_state.rs
```

Change the fallback loop to count whether any stubs were appended:

```rust
let mut declarations = std::mem::take(&mut module_symbols.declarations);

#[cfg(debug_assertions)]
let declaration_count_before_stub_backfill = declarations.len();

for stub in module_symbols.declaration_stubs_by_path.values() {
    if declarations
        .iter()
        .any(|declaration| declaration.id == stub.path)
    {
        continue;
    }

    declarations.push(stub.declaration.to_owned());
}

#[cfg(debug_assertions)]
debug_assert_eq!(
    declarations.len(),
    declaration_count_before_stub_backfill,
    "dependency sorting should now build a complete declaration placeholder list; \
     declaration_stubs_by_path should not backfill AST declarations"
);
```

### Why this intermediate step is useful

It catches missing declaration kinds before the fallback is deleted.

If this assertion fails, inspect which stub kind was missing. The expected fix is in `build_sorted_declarations`, not in AST.

### Validation after Step 3

Run focused tests first:

```bash
cargo test constant
cargo run tests -- constant_soft_dependency_resolves_after_placeholder
cargo run tests -- constant_cross_file_soft_dependency
cargo run tests -- struct_placeholder_visible_before_resolution
```

If the integration runner does not support direct case filtering with that exact syntax, run the smallest supported tag/case filter available. Otherwise run:

```bash
cargo run tests
```

Then run:

```bash
cargo test
```

Do not proceed to deletion until the assertion is silent.

---

## Step 4 — Replace constant deferral's dependency on `DeclarationStub`

Edit:

```text
src/compiler_frontend/ast/module_ast/pass_type_resolution.rs
```

### Current imports

Current code imports:

```rust
use crate::compiler_frontend::headers::module_symbols::{DeclarationStub, DeclarationStubKind};
```

Remove those imports.

### Build a constant header path set

Inside `resolve_constant_headers`, before the `while !pending_headers.is_empty()` loop or at the start of the function, build:

```rust
let constant_header_paths = sorted_headers
    .iter()
    .filter(|header| matches!(header.kind, HeaderKind::Constant { .. }))
    .map(|header| header.tokens.src_path.to_owned())
    .collect::<FxHashSet<_>>();
```

This requires no new data stored in `ModuleSymbols`.

### Change the call site

Current code:

```rust
if is_deferrable_constant_resolution_error(
    &error,
    visible_symbol_paths,
    &self.module_symbols.declaration_stubs_by_path,
    string_table,
)
```

Change to:

```rust
if is_deferrable_constant_resolution_error(
    &error,
    visible_symbol_paths,
    &constant_header_paths,
    string_table,
)
```

### Change helper signature

Current helper:

```rust
fn is_deferrable_constant_resolution_error(
    error: &CompilerError,
    visible_symbol_paths: &FxHashSet<InternedPath>,
    declaration_stubs_by_path: &FxHashMap<InternedPath, DeclarationStub>,
    string_table: &mut StringTable,
) -> bool
```

New helper:

```rust
fn is_deferrable_constant_resolution_error(
    error: &CompilerError,
    visible_symbol_paths: &FxHashSet<InternedPath>,
    constant_header_paths: &FxHashSet<InternedPath>,
    string_table: &mut StringTable,
) -> bool
```

Current implementation:

```rust
visible_symbol_paths
    .iter()
    .filter(|path| path.name() == Some(variable_id))
    .filter_map(|path| declaration_stubs_by_path.get(path))
    .any(|stub| matches!(stub.kind, DeclarationStubKind::Constant))
```

New implementation:

```rust
visible_symbol_paths
    .iter()
    .filter(|path| path.name() == Some(variable_id))
    .any(|path| constant_header_paths.contains(path))
```

### Why this is better

The helper only needs to answer:

> Is this unresolved visible symbol a constant header that may resolve later?

It does not need a full `DeclarationStub` with a full `Declaration` payload.

---

## Step 5 — Delete `declaration_stubs_by_path`

Edit:

```text
src/compiler_frontend/headers/module_symbols.rs
```

Remove these fields/types/functions:

```rust
pub(crate) declaration_stubs_by_path: FxHashMap<InternedPath, DeclarationStub>,
```

```rust
pub(crate) fn seed_declaration_stubs(...)
```

```rust
pub(crate) enum DeclarationStubKind
```

```rust
pub(crate) struct DeclarationStub
```

But do **not** delete the declaration-construction helper entirely yet. It is still useful for `build_sorted_declarations`.

Rename:

```rust
fn declaration_stub_from_header(...)
```

to:

```rust
fn declaration_from_header(...)
```

and make it return:

```rust
Option<Declaration>
```

Instead of returning a `DeclarationStub`.

### Before

```rust
fn declaration_stub_from_header(
    header: &Header,
    string_table: &mut StringTable,
) -> Option<DeclarationStub>
```

### After

```rust
fn declaration_from_header(
    header: &Header,
    string_table: &mut StringTable,
) -> Option<Declaration>
```

Then `build_sorted_declarations` becomes:

```rust
pub(crate) fn build_sorted_declarations(
    &mut self,
    sorted_headers: &[Header],
    string_table: &mut StringTable,
) {
    self.declarations.clear();

    for header in sorted_headers {
        if let Some(declaration) = declaration_from_header(header, string_table) {
            self.declarations.push(declaration);
        }
    }

    self.declarations.append(&mut self.builtin_declarations);
}
```

### Mapping old declaration construction to new helper

| Header kind | New return |
|---|---|
| `Function` | `Some(Declaration { id: header.tokens.src_path, DataType::Function(...) })` |
| `Constant` | `Some(constant_declaration_placeholder(...))` |
| `Struct` | `Some(Declaration { id: header.tokens.src_path, DataType::runtime_struct(...) })` |
| `Choice` | `Some(Declaration { id: header.tokens.src_path, DataType::Choices { ... } })` |
| `StartFunction` | `Some(Declaration { id: source_file/start, DataType::Function(...) })` |
| `ConstTemplate` | `None` |

Rename:

```rust
fn constant_declaration_stub(...)
```

to:

```rust
fn constant_declaration_placeholder(...)
```

This name is more accurate because AST later resolves the real constant value.

### Update comments

Update the top-level module doc from:

> defines `ModuleSymbols`, the top-level symbol collection built during header parsing and finalized (sorted declarations) during dependency sorting.

to:

> defines `ModuleSymbols`, the header-owned symbol metadata package built during header parsing. Dependency sorting fills its complete sorted declaration placeholder list.

Remove wording like “every top-level declaration stub” where it implies the separate stub map still exists.

---

## Step 6 — Delete AST fallback declaration seeding

Edit:

```text
src/compiler_frontend/ast/module_ast/build_state.rs
```

Replace:

```rust
let mut declarations = std::mem::take(&mut module_symbols.declarations);
for stub in module_symbols.declaration_stubs_by_path.values() {
    if declarations
        .iter()
        .any(|declaration| declaration.id == stub.path)
    {
        continue;
    }
    declarations.push(stub.declaration.to_owned());
}
```

with:

```rust
let declarations = std::mem::take(&mut module_symbols.declarations);
```

Update the comment above `declarations` field if needed.

Current comment:

```rust
// Starts as manifest declaration stubs; grows with resolved constants and struct types
// in passes 3–4. Separate from manifest because it is mutated during AST construction.
```

Recommended comment:

```rust
// Starts as dependency-sorted top-level declaration placeholders produced by
// resolve_module_dependencies; grows with resolved constants and struct types
// during AST type resolution.
```

This keeps the model precise without calling it a “manifest.”

---

## Step 7 — Remove `seed_declaration_stubs` call from header parsing

Edit:

```text
src/compiler_frontend/headers/parse_file_headers.rs
```

Current `build_module_symbols` ends with:

```rust
module_symbols.seed_declaration_stubs(headers, string_table);
```

Remove it.

The header stage should still build:

- `module_file_paths`
- `canonical_source_by_symbol_path`
- `file_imports_by_source`
- `importable_symbol_exported`
- `declared_paths_by_file`
- `declared_names_by_file`
- builtin visible symbols
- builtin declarations
- builtin struct metadata

Dependency sorting will build declarations later.

---

## Step 8 — Update dependency-sort comments and naming

Edit:

```text
src/compiler_frontend/module_dependencies.rs
```

Current docs say dependency sorting finalizes `ModuleSymbols.declarations`. Keep that, but make it explicit that the declarations list is **complete**.

Suggested doc text:

```rust
/// WHAT: the output of `resolve_module_dependencies` — sorted headers and a
/// `ModuleSymbols` whose `declarations` Vec contains every top-level declaration
/// placeholder in dependency/start order, plus builtin declarations.
```

Also update comment near `build_sorted_declarations`:

```rust
// Build the complete sorted declaration placeholder list from the topologically
// ordered headers and append builtins.
```

Do not change dependency resolution behavior in this phase unless start-function graph cleanup was explicitly moved from Phase 1. If that cleanup remains pending, leave it for a separate commit.

---

## Step 9 — Search-and-delete stale references

Run these searches:

```bash
rg "declaration_stubs_by_path"
rg "DeclarationStub"
rg "DeclarationStubKind"
rg "seed_declaration_stubs"
rg "declaration_stub_from_header"
rg "constant_declaration_stub"
```

Expected after cleanup:

- no `declaration_stubs_by_path`
- no `DeclarationStub`
- no `DeclarationStubKind`
- no `seed_declaration_stubs`
- no `declaration_stub_from_header`
- `constant_declaration_placeholder` may remain

Search for comments too, not only code.

---

## Step 10 — Validation sequence

Because this phase touches frontend state passed into many features, validation should be layered.

### 10.1 Fast compile checks

```bash
cargo check
```

Then:

```bash
cargo test module_dependencies
cargo test parse_file_headers
cargo test pass_type_resolution
cargo test import_bindings
```

Some exact module test filters may need adjustment depending on test names.

### 10.2 Focused integration cases

Run focused constants/imports/structs cases:

```bash
cargo run tests -- constants
cargo run tests -- constant
cargo run tests -- imports
cargo run tests -- structs
cargo run tests -- dependency
```

If the runner does not support tag filtering this way, run the full integration runner:

```bash
cargo run tests
```

### 10.3 Full validation

```bash
cargo test
cargo run tests
cargo clippy
```

Then optional but recommended:

```bash
cargo run --features "detailed_timers" docs
cargo run --release --features "detailed_timers" speed-test.bst
```

Run `cargo fmt` only after the above pass.

---

# Expected diff summary

A good Phase 2 diff should look like this:

## `module_symbols.rs`

Expected changes:

- remove `declaration_stubs_by_path`
- remove `DeclarationStub`
- remove `DeclarationStubKind`
- remove `seed_declaration_stubs`
- rename `declaration_stub_from_header` to `declaration_from_header`
- make `build_sorted_declarations` push all declaration placeholders
- rename `constant_declaration_stub` to `constant_declaration_placeholder`
- update module and field docs

Expected behavior preserved:

- builtin declarations still appended
- const templates still not declaration placeholders
- start function still represented as a declaration placeholder after sorted top-level declarations

## `build_state.rs`

Expected changes:

- remove fallback loop over `module_symbols.declaration_stubs_by_path`
- update declaration field comments

Expected behavior preserved:

- `declarations` still starts populated before AST passes
- builtin struct AST nodes still extracted
- resolved struct fields/source maps still extracted

## `pass_type_resolution.rs`

Expected changes:

- remove `DeclarationStub` and `DeclarationStubKind` imports
- build `constant_header_paths`
- update deferrable constant helper signature and implementation

Expected behavior preserved:

- constants still resolve in fixed-point rounds
- unresolved visible constant references still defer
- actual non-constant expressions still error
- cross-file import visibility still enforced

## `parse_file_headers.rs`

Expected changes:

- remove `module_symbols.seed_declaration_stubs(headers, string_table)`
- possibly update comments that mention declaration stubs if Phase 1 split kept these comments nearby

Expected behavior preserved:

- header parsing still owns symbol discovery
- header parsing still does not fill sorted declarations
- builtin registration still happens here

## Tests

Expected additions:

- constant soft dependency case
- cross-file constant soft dependency case
- struct placeholder visibility case, unless already covered strongly

---

# Risk analysis

## Risk 1 — Constants stop seeing unresolved placeholders

### Cause

`build_sorted_declarations` does not include constant placeholders, or resolved constants are not visible when expected.

### Guard

- `constant_soft_dependency_resolves_after_placeholder`
- `constant_cross_file_soft_dependency`

### Fix

Ensure `declaration_from_header` returns a constant placeholder for `HeaderKind::Constant`.

---

## Risk 2 — Struct references break before struct field resolution

### Cause

Struct placeholders no longer enter initial `declarations`.

### Guard

- struct constructor / function return tests
- new `struct_placeholder_visible_before_resolution` case

### Fix

Ensure `declaration_from_header` returns a `DataType::runtime_struct` placeholder for `HeaderKind::Struct`.

---

## Risk 3 — Duplicate declaration behavior changes

### Cause

Changing placeholder ordering could affect `TopLevelDeclarationIndex::get_visible`, especially because `DeclarationBucket::Many` walks indices in reverse.

### Guard

Existing tests for:

- duplicate function names
- duplicate struct names
- import name collision
- shadowing rejection

### Fix

Preserve current sorted order as closely as possible:
1. sorted top-level declaration placeholders in sorted header order
2. appended start function
3. builtins

Do not switch to map-based declaration storage in this phase.

---

## Risk 4 — Builtin error types disappear or resolve differently

### Cause

`builtin_declarations` not appended or moved too early/late.

### Guard

Existing builtin error reserved/usage tests.

### Fix

Keep `self.declarations.append(&mut self.builtin_declarations)` inside `build_sorted_declarations`.

---

## Risk 5 — Constant deferral becomes too permissive

### Cause

New `constant_header_paths` check might defer unresolved variable names that are visible but not actually constants.

### Guard

Tests for undeclared variables, non-constant references, and function references inside constants.

### Fix

The helper must require both:
1. visible symbol name matches the unresolved variable
2. matching path is in `constant_header_paths`

Do not defer based on name alone.

---

# Review checklist

Before merging Phase 2, verify:

- [ ] `rg "declaration_stubs_by_path"` returns nothing
- [ ] `rg "DeclarationStub"` returns nothing
- [ ] `rg "DeclarationStubKind"` returns nothing
- [ ] `AstBuildState::new` only takes `module_symbols.declarations`
- [ ] `build_sorted_declarations` includes constants and structs
- [ ] `HeaderKind::ConstTemplate` still returns no declaration placeholder
- [ ] constant deferral uses `constant_header_paths`
- [ ] no new AST pass reconstructs declarations from headers
- [ ] comments no longer describe a separate declaration-stub map
- [ ] integration tests for constants/imports/structs pass
- [ ] full `cargo run tests` passes

---

# Optional follow-up after Phase 2

Do **not** include this in Phase 2 unless the main cleanup is already stable.

## Replace resolved constants/structs instead of appending

Current behavior appears to seed placeholder declarations, then append resolved constants/structs later. This relies on `TopLevelDeclarationIndex::get_visible` choosing the latest visible declaration when multiple declarations share a name.

A future cleanup could introduce:

```rust
fn replace_declaration_by_id(
    declarations: &mut Vec<Declaration>,
    id: &InternedPath,
    replacement: Declaration,
)
```

Then constant and struct resolution would replace placeholders instead of appending resolved forms.

This would reduce duplicate declarations, but it is a semantic-sensitive change because many lookups currently depend on order and reverse lookup. Keep it separate.

---

# Final target state

After Phase 2, the pipeline should read cleanly:

1. `parse_headers`
   - parses declaration shells
   - builds symbol/import/export/source metadata
   - stages builtin metadata
   - leaves `ModuleSymbols.declarations` empty

2. `resolve_module_dependencies`
   - sorts strict top-level declaration headers
   - appends `StartFunction`
   - builds complete sorted declaration placeholders
   - appends builtin declarations

3. `Ast::new`
   - receives sorted headers and complete `ModuleSymbols`
   - takes `module_symbols.declarations`
   - resolves constants/structs/functions against that declaration list
   - never backfills or rebuilds top-level declaration stubs

This makes the docs and code match the intended compiler contract more tightly: header parsing owns discovery, dependency sorting owns order, AST owns semantic resolution.
