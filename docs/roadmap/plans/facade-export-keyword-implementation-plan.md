# Beanstalk Module Facade Export Keyword Implementation Plan

## Goal

Refactor module facades so `#mod.bst` explicitly controls the module API with the new reserved `export` keyword.

The target language rule is:

> `export` is reserved everywhere, but valid only in `#mod.bst`. In a module facade, `export` marks the following top-level header as part of the module API. Unmarked top-level headers and regular imports are private to the facade file.

This replaces the current automatic `#mod.bst` authored-header export behavior with explicit facade exports, while keeping module internals simple and avoiding a general per-file visibility system.

## Final language surface

```beanstalk
-- #mod.bst

import @./theme {
    spacing,
}

private_prefix #= "ui"

Private_Config = |
    size String,
|

make_private_label |text String| -> String:
    return [: [private_prefix]-[text]]
;

export Button = |
    label String,
|

export render |button Button| -> String:
    return [:
        <button>[button.label]</button>
    ]
;

export import @./card {
    Card,
    render_card as render,
}

export @./layout {
    page,
    section,
}
```

`export @./layout { page }` is syntax sugar for:

```beanstalk
export import @./layout {
    page,
}
```

## Required semantics

- `export` is a reserved keyword in all Beanstalk source.
- `export` is valid only in `#mod.bst`.
- `#mod.bst` remains a facade declaration file, not a runtime entry file.
- Runtime top-level statements, runtime templates, and top-level page fragments remain invalid in `#mod.bst`.
- Unmarked top-level declarations in `#mod.bst` are private to that file.
- Public facade declarations must use `export`.
- Regular `import` in `#mod.bst` is private to the facade file.
- `export import @path { Symbol }` and `export @path { Symbol }` re-export imported symbols through the module facade.
- Re-export aliases define the public API name.
- `export @path` without a grouped explicit symbol list is deferred; do not add namespace exports in v1.
- Wildcard exports remain unsupported.
- `export` must not be accepted in ordinary `.bst`, `#page.bst`, or `#config.bst`.
- Direct imports of `#mod.bst`, `#page.bst`, and `#config.bst` remain invalid.
- Exported signatures must not expose private facade-only types.

## Non-goals

- No wildcard imports or exports.
- No public namespace records.
- No `export` in ordinary implementation files.
- No general `pub`/`private` visibility system.
- No compatibility path for the old automatic facade export behavior.
- No legacy `#import`.
- No package-manager, versioning, remote fetching, or lockfile changes.
- No changes to runtime start execution semantics.

## Current repo anchors

These are the main implementation anchors to inspect and change.

| Area | Current path | Why it matters |
|---|---|---|
| Keyword mapping | `src/compiler_frontend/keywords.rs` | Adds `export` as a reserved keyword and tokenizer keyword. |
| Token model | `src/compiler_frontend/tokenizer/tokens.rs` | Adds `TokenKind::Export`. |
| Header top-level classification | `src/compiler_frontend/headers/top_level_classifier.rs` | Routes top-level `export` to the header parser instead of start-body handling. |
| File header parser | `src/compiler_frontend/headers/file_parser.rs` | Owns the file-level header state machine and should parse `export` headers. |
| Import clause recording | `src/compiler_frontend/headers/file_imports.rs` | Needs import visibility/export metadata for private imports vs re-exports. |
| Header data contracts | `src/compiler_frontend/headers/types.rs` | Add explicit export mode on parsed headers and file imports. |
| Per-file header state | `src/compiler_frontend/headers/file_state.rs` | Move file imports into per-file output so import-only facades work. |
| Module symbol collection | `src/compiler_frontend/headers/symbol_collection.rs` | Register all file-level imports once and stop relying on imports copied into every header. |
| Facade export maps | `src/compiler_frontend/headers/facade_data.rs` | Replace automatic authored-header facade export with explicit `export` exports and re-exports. |
| Import environment | `src/compiler_frontend/headers/import_environment/` | Resolve facade exports, re-export targets, receiver method visibility, and public/private import behavior. |
| Import path clause parser | `src/compiler_frontend/paths/const_paths/import_clauses.rs` | Reuse/extend import parsing for `export @path { ... }` sugar and Stage 0 reachability scanning. |
| Stage 0 path collection | `src/compiler_frontend/paths/const_paths/import_clauses.rs` | Must collect paths from `export @path { ... }`, not only `import @path`. |
| Docs | `docs/language-overview.md`, `docs/compiler-design-overview.md`, `docs/src/docs/progress/#page.bst`, `docs/src/docs/**` | Update the language contract, compiler-stage contract, matrix, and user-facing module/import docs. |

## Design choices for implementation

### Use an enum, not a bare bool

Add a named export mode:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderExportMode {
    Private,
    Public,
}
```

Use this on both declarations and file imports:

```rust
pub struct Header {
    pub export_mode: HeaderExportMode,
    ...
}

pub struct FileImport {
    pub export_mode: HeaderExportMode,
    ...
}
```

`Private` means private to the source file/importing file. `Public` means the `#mod.bst` facade intends to expose that header or import through the module API.

### Keep ordinary module internals simple

Normal files should continue to expose declarations internally to the module as they do today. Do not add `export` to ordinary files.

`export` is a facade API marker, not a general source-file visibility marker.

### Store imports once per file

The current `Header` type carries `file_imports`, which duplicates the same file import list onto every header from that file. This becomes more fragile with import-only re-export facades.

Refactor file imports into per-file output metadata and keep headers focused on declarations:

```rust
pub struct FileFrontendPrepareOutput {
    pub source_file: InternedPath,
    pub file_role: FileRole,
    pub file_imports: Vec<FileImport>,
    ...
}
```

Then build `ModuleSymbols.file_imports_by_source` from per-file outputs, not by merging import copies from each header.

This is also required so a valid facade containing only re-exports still contributes imports and facade data:

```beanstalk
-- #mod.bst
export @./button { Button }
export @./card { Card }
```

### Treat `export @path` as import sugar only

`export @./x { A }` should parse into the same `FileImport` records as `export import @./x { A }`, with `export_mode: Public`.

Reject `export @./x` in v1 because that is a namespace export, not a symbol re-export.

### Keep facade export maps as the public API source of truth

`facade_data.rs` should build public API maps from:

1. public authored headers in `#mod.bst`;
2. public import records in `#mod.bst`.

Everything else remains private.

### Support external re-exports deliberately

`FacadeExportTarget` already documents that a facade export target may be source or external, but currently only stores source targets. Extend it:

```rust
pub enum FacadeExportTarget {
    Source(InternedPath),
    External(ExternalSymbolId),
}
```

This lets a module facade expose builder/core/external package symbols consistently when intentionally re-exported.

### Receiver methods need explicit facade discipline

Internal imports should keep current behavior: importing a receiver type inside the same module/library can see same-source receiver methods.

Cross-facade imports should not leak private implementation receiver methods accidentally.

For v1, use this rule:

- Public receiver methods are visible through a facade only when the facade explicitly exports the receiver method or exports a wrapper.
- Importing a receiver type through a facade may auto-import only receiver methods that are also exported by that same facade surface.
- Re-exporting a receiver method still requires the receiver type to be visible/exported through the same grouped import, an earlier public facade entry, or a transparent public alias.

Example:

```beanstalk
-- #mod.bst
export @./button {
    Button,
    render,
}
```

Consumers should be able to call `button.render()` after importing `Button` from the facade only if `render` is part of the facade export set.

## Phase 0 — Baseline audit and fixture inventory

### Context

Start by mapping current facade behavior before changing code. The current implementation automatically exports authored declarations from `#mod.bst`; the new design makes exports explicit and allows private facade helpers.

### Checklist

- [ ] Inspect all current facade/import code paths:
  - [ ] `src/compiler_frontend/keywords.rs`
  - [ ] `src/compiler_frontend/tokenizer/tokens.rs`
  - [ ] `src/compiler_frontend/headers/top_level_classifier.rs`
  - [ ] `src/compiler_frontend/headers/file_parser.rs`
  - [ ] `src/compiler_frontend/headers/file_imports.rs`
  - [ ] `src/compiler_frontend/headers/file_state.rs`
  - [ ] `src/compiler_frontend/headers/types.rs`
  - [ ] `src/compiler_frontend/headers/symbol_collection.rs`
  - [ ] `src/compiler_frontend/headers/facade_data.rs`
  - [ ] `src/compiler_frontend/headers/import_environment/**`
  - [ ] `src/compiler_frontend/paths/const_paths/import_clauses.rs`
- [ ] Search for all uses of:
  - [ ] `FileRole::ModuleFacade`
  - [ ] `facade_exports`
  - [ ] `module_root_facade_exports`
  - [ ] `FacadeExportEntry`
  - [ ] `FacadeExportTarget`
  - [ ] `importable_symbol_exported`
  - [ ] `not_exported_by_facade`
  - [ ] `not_exported_by_source_file`
  - [ ] `direct_special_file_import`
- [ ] Inventory integration cases under `tests/cases/**` that mention:
  - [ ] `#mod.bst`
  - [ ] source library facades
  - [ ] module-root facades
  - [ ] direct special-file imports
  - [ ] grouped receiver-method imports
  - [ ] namespace imports through facades
- [ ] Record which tests currently depend on automatic `#mod.bst` export.
- [ ] Decide whether old fixtures should be updated to explicit `export` or inverted into negative tests proving unmarked facade headers are private.

### Phase-end audit / style review / validation

- [ ] Confirm no code changes are included in this phase unless they are test comments or notes.
- [ ] Run the current validation command to establish baseline:
  - [ ] `just validate`
- [ ] If baseline is not green, record the existing failures before continuing.
- [ ] Confirm the implementation scope still excludes wildcard exports and namespace exports.

## Phase 1 — Reserve and tokenize `export`

### Context

`export` should be reserved across the language, even though it is only valid in `#mod.bst`. The tokenizer and identifier policy are the right ownership boundary for this.

### Checklist

- [ ] Add `TokenKind::Export` to `src/compiler_frontend/tokenizer/tokens.rs`.
  - [ ] Place it near `Import` because both are module/header-surface keywords.
  - [ ] Update comments to say `export` is a facade-only API marker.
- [ ] Add `"export"` to `RESERVED_KEYWORD_SHADOWS` in `src/compiler_frontend/keywords.rs`.
  - [ ] Update the array length.
- [ ] Add `"export" => Some(TokenKind::Export)` to `keyword_token_kind`.
- [ ] Ensure identifier validation rejects declarations, aliases, generic parameters, and imports named `export` through existing keyword-shadowing helpers.
- [ ] Add or update tokenizer/keyword tests:
  - [ ] `export` lexes as `TokenKind::Export`.
  - [ ] `export` cannot be used as a top-level declaration name.
  - [ ] `export` cannot be used as an import alias.
  - [ ] `export` cannot be used as a generic parameter name.
- [ ] Confirm template body text containing `export` is unaffected because template bodies are raw/text-tokenized.

### Phase-end audit / style review / validation

- [ ] Confirm no parser accepts `export` yet except as a token.
- [ ] Confirm keyword policy is centralized in `keywords.rs`; do not add ad hoc string checks elsewhere.
- [ ] Run:
  - [ ] `cargo fmt`
  - [ ] focused tokenizer/keyword tests
  - [ ] `just validate`

## Phase 2 — Refactor header data around explicit export mode and per-file imports

### Context

The current header shape copies file imports onto every header. This creates redundancy and fails the import-only facade use case. Before parsing `export`, make the data model capable of representing private/public headers and imports once per file.

### Checklist

- [ ] Add `HeaderExportMode` to `src/compiler_frontend/headers/types.rs`.
  - [ ] Include file-level doc comments explaining `Private` and `Public`.
  - [ ] Add small helpers only if they improve readability, for example `is_public()`.
- [ ] Add `export_mode: HeaderExportMode` to `Header`.
- [ ] Add `export_mode: HeaderExportMode` to `FileImport`.
- [ ] Add `file_role: FileRole` to `FileFrontendPrepareOutput`.
- [ ] Add `file_imports: Vec<FileImport>` to `FileFrontendPrepareOutput`.
- [ ] Add `canonical_os_path: Option<PathBuf>` or a small file metadata struct if needed by facade membership logic without relying on a declaration header.
- [ ] Update `FileFrontendPrepareOutput::remap_string_ids` to remap `file_imports`.
- [ ] Update `Header::remap_string_ids` to stop remapping imports if `Header.file_imports` is removed.
- [ ] Remove `file_imports` from `Header` unless a real downstream consumer still needs it.
- [ ] Keep `HeaderBuildContext.file_import_entries` as a borrowed slice so dependency-edge collection can still inspect current file imports without storing them on the header.
- [ ] Update `HeaderFileParseState::into_non_entry_output` and `into_entry_output` to store file imports on `FileFrontendPrepareOutput`.
- [ ] Update the implicit start `Header` construction with `export_mode: HeaderExportMode::Private`.
- [ ] Update `create_header` to receive an explicit `HeaderExportMode` and store it on `Header`.
- [ ] Update all existing `create_header` call sites to pass `HeaderExportMode::Private`.
- [ ] Refactor `build_module_symbols` so it receives per-file metadata as well as headers.
  - [ ] Register `module_file_paths` for every prepared file, including import-only files.
  - [ ] Register `file_imports_by_source` from per-file outputs, not by merging import copies from headers.
  - [ ] Keep declaration registration driven by headers.
- [ ] Delete `merge_header_imports` if it becomes obsolete.
- [ ] Keep `register_declared_symbol` focused on declared path/name registration.
  - [ ] Do not overload `HeaderExportMode` into the old `importable_symbol_exported` map.
  - [ ] Rename `importable_symbol_exported` only if it can be done cleanly in this phase; otherwise defer the rename to the cleanup phase.
- [ ] Update existing tests affected by the data-shape change without changing language behavior yet.

### Phase-end audit / style review / validation

- [ ] Confirm imports are stored exactly once per source file.
- [ ] Confirm import-only files now appear in module file metadata.
- [ ] Confirm no stale comments still say every header carries file imports.
- [ ] Confirm no compatibility wrappers preserve both old and new import storage paths.
- [ ] Run:
  - [ ] `cargo fmt`
  - [ ] focused header remap tests
  - [ ] focused import/facade tests
  - [ ] `just validate`

## Phase 3 — Parse facade-only `export` headers

### Context

This phase introduces source syntax but should not yet rely on the final facade export map logic. The parser should classify `export`, enforce `#mod.bst`-only usage, and mark the following declaration/import as public.

### Checklist

- [ ] Add `HeaderFileItem::Export` to `top_level_classifier.rs`.
- [ ] Classify `TokenKind::Export` as `HeaderFileItem::Export` only when it starts at a statement boundary.
- [ ] Treat non-boundary `export` as a start-body token so normal expression/statement diagnostics handle invalid use.
- [ ] Add `handle_export_item` to `file_parser.rs`.
- [ ] In `handle_export_item`, reject immediately unless `context.file_role == FileRole::ModuleFacade`.
  - [ ] Add a structured diagnostic such as `CompilerDiagnostic::export_outside_module_facade`.
  - [ ] Message should say: `export` is only valid in `#mod.bst`; expose declarations through the nearest module facade.
- [ ] After `export`, require the next significant token to be on the same logical header.
  - [ ] Do not allow `export` alone on a line to apply to the next line.
  - [ ] Emit a specific diagnostic for missing export target.
- [ ] Support `export import @path { ... }`.
  - [ ] Parse through the import clause parser.
  - [ ] Store produced `FileImport` entries with `HeaderExportMode::Public`.
- [ ] Support `export @path { ... }` sugar.
  - [ ] Reuse the same path-token import clause validation as `export import`.
  - [ ] Store produced `FileImport` entries with `HeaderExportMode::Public`.
- [ ] Reject `export @path` when the parsed path token is not a grouped symbol list.
  - [ ] Diagnostic should say namespace exports are deferred and suggest `export @path { Symbol }`.
- [ ] Support exported declarations:
  - [ ] `export name #= value`
  - [ ] `export name #Type = value`
  - [ ] `export Name as Type`
  - [ ] `export Struct = | ... |`
  - [ ] `export Choice :: ...`
  - [ ] `export function |...| -> ...: ... ;`
- [ ] Reject `export` before unsupported top-level items:
  - [ ] runtime statements;
  - [ ] runtime templates;
  - [ ] `#[...]` const page fragments;
  - [ ] `#config`-style syntax;
  - [ ] reserved/deferred trait syntax.
- [ ] Preserve current duplicate declaration detection.
  - [ ] `export Foo = |...|` and `Foo = |...|` in the same `#mod.bst` should report duplicate declaration.
  - [ ] `export @./x { Foo }` and `import @./x { Foo }` should collide if they bind different symbols.
- [ ] Update `parse_and_record_imports` into a clearer helper that can record either private or public imports.
  - [ ] Prefer a named input struct over a long parameter list.
  - [ ] Include export mode in duplicate-import identity so a private import and a public re-export of the same symbol can either be normalized into one public record or diagnosed deterministically.
  - [ ] Prefer normalization: same path + same alias + any public occurrence => one public import record.
- [ ] Update `collect_paths_from_tokens` in Stage 0 path collection.
  - [ ] Continue collecting paths from normal `import`.
  - [ ] Collect paths from `export import @path { ... }`.
  - [ ] Collect paths from `export @path { ... }`.
  - [ ] Do not collect anything from exported authored declarations.
- [ ] Add parser-level tests for:
  - [ ] `export` outside `#mod.bst` rejected.
  - [ ] `export` alone rejected.
  - [ ] `export @./x { A }` parsed as a public import.
  - [ ] `export import @./x { A }` parsed as a public import.
  - [ ] `export @./x` rejected as deferred namespace export.
  - [ ] `export` before each supported authored declaration kind marks the header public.
  - [ ] unmarked authored declarations in `#mod.bst` remain private headers.

### Phase-end audit / style review / validation

- [ ] Confirm parsing does not create a second re-export grammar; it reuses import clause parsing.
- [ ] Confirm `export @path` is only sugar for `export import @path`.
- [ ] Confirm no runtime/parser stage accepts `export` outside `#mod.bst`.
- [ ] Confirm diagnostics use `CompilerDiagnostic`, not `CompilerError`.
- [ ] Run:
  - [ ] `cargo fmt`
  - [ ] focused header parser tests
  - [ ] focused Stage 0 import path collection tests
  - [ ] `just validate`

## Phase 4 — Replace automatic facade exports with explicit export maps

### Context

The current `facade_data.rs` auto-exports every authored declaration from `#mod.bst`. This phase makes the explicit `export` mode the only source of public facade API entries and wires re-exported imports into the same map.

### Checklist

- [ ] Replace `is_authored_facade_export` with an explicit public check:
  - [ ] `header.file_role == FileRole::ModuleFacade`
  - [ ] `header.export_mode == HeaderExportMode::Public`
  - [ ] header kind is an exportable authored declaration
- [ ] Update comments in `facade_data.rs` that currently say every authored `#mod.bst` declaration is automatically exported.
- [ ] Build source-library facade exports from:
  - [ ] public authored headers in the source-library `#mod.bst`;
  - [ ] public import records in that `#mod.bst`.
- [ ] Build module-root facade exports from:
  - [ ] public authored headers in the module-root `#mod.bst`;
  - [ ] public import records in that `#mod.bst`.
- [ ] Ensure import-only facades produce non-empty facade exports.
- [ ] Extend `FacadeExportTarget` to include external symbols:
  - [ ] `Source(InternedPath)`
  - [ ] `External(ExternalSymbolId)`
- [ ] Extend `FacadeLookupResult` to distinguish source vs external exports:
  - [ ] `ExportedSource { path, surface }` or equivalent;
  - [ ] `ExportedExternal { symbol_id }`;
  - [ ] keep `NotExported` and `NotAFacadeImport`.
- [ ] Update `resolve_facade_import` and callers.
  - [ ] Source facade imports should register source imports.
  - [ ] External facade imports should register external imports.
- [ ] Add a small facade export resolver for public imports in `facade_data.rs`.
  - [ ] Reuse `resolve_external_package_symbol` for external package re-exports.
  - [ ] Reuse `resolve_facade_import` when a facade re-exports another facade’s public symbol.
  - [ ] Reuse `resolve_import_target` for same-module/same-library source re-exports.
  - [ ] Apply module-boundary checks when public imports target another module root.
- [ ] Public export name rules:
  - [ ] alias wins: `export @./x { render_button as render }` exports `render`;
  - [ ] otherwise use the imported symbol name;
  - [ ] reject missing local/export names.
- [ ] Add duplicate public export detection.
  - [ ] Two public exports with the same exported name and same target may be accepted only if this keeps the implementation simpler and deterministic.
  - [ ] Two public exports with the same exported name and different targets must be rejected.
  - [ ] Prefer rejecting duplicate public export names always for clearer facade APIs.
- [ ] Add collision diagnostics for public authored header vs public re-export with the same exported name.
- [ ] Keep private `#mod.bst` imports available to exported wrapper declarations through normal file visibility.
- [ ] Keep plain private `#mod.bst` declarations visible only inside `#mod.bst`.
- [ ] Keep direct imports of special files rejected.

### Phase-end audit / style review / validation

- [ ] Confirm `#mod.bst` authored declarations are no longer exported unless explicitly public.
- [ ] Confirm public re-export entries use the same facade export map as authored declarations.
- [ ] Confirm source-library and module-root facade logic stay structurally parallel.
- [ ] Confirm there is no duplicate target-resolution code that should be extracted into a named helper.
- [ ] Run:
  - [ ] `cargo fmt`
  - [ ] focused facade resolution tests
  - [ ] focused source-library import tests
  - [ ] focused module-root import tests
  - [ ] `just validate`

## Phase 5 — Public type-surface and receiver-method validation

### Context

Explicit exports make `#mod.bst` a true API boundary. The compiler should reject public signatures that require private names the importer cannot see.

### Checklist

- [ ] Define a public-nameability model for facade APIs.
  - [ ] Builtin scalar/string/error/option/collection types are public.
  - [ ] Generic parameters declared on the exported declaration are public within that declaration.
  - [ ] Public authored facade types are public.
  - [ ] Public re-exported types are public under their exported names.
  - [ ] Private imports are not public unless also re-exported.
  - [ ] Private facade declarations are not public unless marked `export`.
- [ ] Add validation for public authored declarations in `#mod.bst`.
  - [ ] Function parameter types.
  - [ ] Function return types.
  - [ ] Struct field types.
  - [ ] Choice payload field types.
  - [ ] Type alias targets.
  - [ ] Constant explicit type annotations.
- [ ] Add a follow-up AST/finalize validation for exported constants whose type is inferred from the initializer.
  - [ ] Reject inferred private nominal/private alias/external-private types in public constants.
  - [ ] Keep diagnostics typed and source-located.
- [ ] Add a structured diagnostic such as `private_type_in_exported_api`.
  - [ ] Include the exported declaration name.
  - [ ] Include the private type/name.
  - [ ] Suggest `export @path { TypeName }` or hiding the type behind a public wrapper.
- [ ] Receiver method behavior:
  - [ ] Internal imports keep current same-source receiver method auto-import behavior.
  - [ ] Cross-facade imports auto-import only receiver methods exported by the same facade surface.
  - [ ] Explicit grouped receiver-method re-exports require the receiver type to be public in the same facade surface.
  - [ ] Type aliases in a facade must not accidentally expose private implementation methods.
- [ ] Update `register_source_import` and related receiver auto-import helpers to know whether an import came from:
  - [ ] internal resolution;
  - [ ] direct source resolution requiring source-file export;
  - [ ] facade resolution with a known public surface.
- [ ] Prefer replacing `ExportRequirement` with a clearer enum if needed:
  - [ ] `SourceImportAccess::Internal`
  - [ ] `SourceImportAccess::DirectSourceExport`
  - [ ] `SourceImportAccess::Facade { exported_entries: ... }`
- [ ] Update receiver-method tests around facade imports.

### Phase-end audit / style review / validation

- [ ] Confirm public API leak validation happens at the earliest stage with enough semantic facts.
- [ ] Confirm no check clones full `DataType` values for semantic comparison if `TypeId` is available.
- [ ] Confirm diagnostics carry structured facts and source locations.
- [ ] Confirm receiver method visibility does not bypass explicit facade exports.
- [ ] Run:
  - [ ] `cargo fmt`
  - [ ] focused public API leak diagnostics
  - [ ] focused receiver method import/re-export tests
  - [ ] `just validate`

## Phase 6 — Integration tests and migration of existing fixtures

### Context

After the implementation works, update the test corpus so the new facade contract is canonical and old automatic export behavior is intentionally rejected.

### Checklist

- [ ] Update existing success fixtures that rely on automatic `#mod.bst` exports to use `export`.
- [ ] Add positive integration cases:
  - [ ] exported function from `#mod.bst`;
  - [ ] exported struct from `#mod.bst`;
  - [ ] exported choice from `#mod.bst`;
  - [ ] exported type alias from `#mod.bst`;
  - [ ] exported compile-time constant from `#mod.bst`;
  - [ ] private helper function used by exported facade wrapper;
  - [ ] private helper type used only inside `#mod.bst`;
  - [ ] private import used by exported facade wrapper;
  - [ ] `export import @./x { A }`;
  - [ ] `export @./x { A }`;
  - [ ] public alias re-export;
  - [ ] import-only facade made of re-exports;
  - [ ] source-library facade re-export;
  - [ ] module-root facade re-export;
  - [ ] external package symbol re-export if Phase 4 supports external targets.
- [ ] Add negative integration cases:
  - [ ] unmarked `#mod.bst` declaration is not importable from outside the module;
  - [ ] unmarked `#mod.bst` import is not public;
  - [ ] `export` in ordinary `.bst` rejected;
  - [ ] `export` in `#page.bst` rejected;
  - [ ] `export` in `#config.bst` rejected;
  - [ ] `export @./x` namespace export rejected;
  - [ ] wildcard export rejected if a wildcard syntax exists or is reserved;
  - [ ] direct import of `#mod.bst` remains rejected;
  - [ ] duplicate public export names rejected;
  - [ ] exported function leaking private parameter type rejected;
  - [ ] exported function leaking private return type rejected;
  - [ ] exported struct leaking private field type rejected;
  - [ ] exported choice leaking private payload type rejected;
  - [ ] exported alias targeting private type rejected;
  - [ ] exported inferred constant with private nominal type rejected, if implemented in Phase 5.
- [ ] Prefer `diagnostic_codes` assertions for negative cases.
- [ ] Add strong output assertions for success cases where the exported function/template output matters.
- [ ] Keep multi-file fixtures inside one case folder so helpers are not treated as standalone tests.

### Phase-end audit / style review / validation

- [ ] Confirm old automatic export behavior has at least one explicit negative regression case.
- [ ] Confirm each new language rule has at least one positive or negative test.
- [ ] Confirm tests use real Beanstalk snippets rather than implementation internals.
- [ ] Run:
  - [ ] `cargo fmt`
  - [ ] `cargo test`
  - [ ] `cargo run -- tests`
  - [ ] `just validate`

## Phase 7 — Documentation updates

### Context

This changes the public module/facade model, so both compiler-facing and user-facing docs must be updated in the same implementation chunk.

### Checklist

- [ ] Update `docs/language-overview.md`.
  - [ ] Add `export` to the syntax summary as a reserved facade-only keyword.
  - [ ] Update the module roots/facades section:
    - [ ] `#mod.bst` can contain private imports and private top-level declarations.
    - [ ] Public headers require `export`.
    - [ ] Regular `import` is private.
    - [ ] `export import @path { Symbol }` re-exports.
    - [ ] `export @path { Symbol }` is sugar for `export import`.
    - [ ] `export @path` namespace export is deferred.
    - [ ] Wildcard exports are deferred.
    - [ ] Direct imports of special files remain invalid.
  - [ ] Remove or rewrite statements saying direct facade re-export syntax is deferred.
  - [ ] Remove or rewrite statements implying every authored declaration in `#mod.bst` is automatically exported.
  - [ ] Add the private type leak rule for exported APIs.
  - [ ] Add receiver-method facade export behavior.
- [ ] Update `docs/compiler-design-overview.md`.
  - [ ] Stage 2 should mention `export` parsing and explicit facade export metadata.
  - [ ] Import/facade contract should say facade exports come from public `#mod.bst` headers and public facade imports.
  - [ ] Header parsing should be described as owning private/public facade metadata.
  - [ ] AST/HIR sections should not imply facade exports are AST wrappers or runtime declarations.
- [ ] Update `docs/src/docs/progress/#page.bst`.
  - [ ] Change module/facade status/watch points to the new explicit `export` model.
  - [ ] Add coverage notes for private facade helpers, re-exports, export-only-in-facade diagnostics, and deferred namespace/wildcard exports.
- [ ] Search `docs/src/docs/**` for module/import/facade pages and update user-facing examples.
  - [ ] Replace automatic `#mod.bst` export examples with explicit `export`.
  - [ ] Add a concise example of private facade helper declarations.
  - [ ] Add a concise example of public re-export sugar.
  - [ ] Add a warning/example showing `import` inside `#mod.bst` is private.
- [ ] Update roadmap/deferred-feature docs if they currently list direct facade re-export syntax as deferred.
  - [ ] Keep wildcard and namespace exports deferred.
  - [ ] Mark explicit facade export/re-export as implemented once complete.

### Phase-end audit / style review / validation

- [ ] Confirm docs and implementation use the same names: `export`, `export import`, `export @path`.
- [ ] Confirm docs do not describe old automatic facade export behavior.
- [ ] Confirm user-facing docs do not over-explain compiler internals.
- [ ] Confirm compiler-facing docs mention the correct stage ownership.
- [ ] Run:
  - [ ] `cargo fmt`
  - [ ] docs/site build command used by the repo, if separate from validation
  - [ ] `just validate`

## Phase 8 — Cleanup, simplification, and final audit

### Context

After behavior and docs are updated, remove stale names and simplify any transitional code introduced during implementation.

### Checklist

- [ ] Search for stale wording:
  - [ ] `reexport`
  - [ ] `re-export syntax deferred`
  - [ ] `automatic facade export`
  - [ ] `every valid authored declaration is automatically exported`
  - [ ] `exported boolean`
  - [ ] `#import`
- [ ] Consider renaming `importable_symbol_exported` if it no longer represents source-level export.
  - [ ] Prefer a name like `importable_source_symbol_paths` plus a separate access model if possible.
  - [ ] Do not do a broad rename if it risks obscuring the main feature diff.
- [ ] Remove obsolete helper functions:
  - [ ] old automatic facade export predicates;
  - [ ] header import merge helpers if imports now live per file;
  - [ ] duplicate import parsing wrappers.
- [ ] Check that direct `#mod.bst` import rejection still happens in one place.
- [ ] Check that `export` diagnostics are not duplicated across parser and AST.
- [ ] Check that `FacadeExportTarget::External` is fully handled by all matches.
- [ ] Check that import-only facades are represented without synthetic headers.
- [ ] Check that private facade helpers do not create public facade entries.
- [ ] Check public aliasing behavior in diagnostics and import maps.
- [ ] Check comments in touched modules:
  - [ ] file-level docs explain the module owner;
  - [ ] comments explain why `export` is facade-only;
  - [ ] no comments restate syntax without intent;
  - [ ] no stale comments mention the old automatic export model.
- [ ] Check tests for bloat.
  - [ ] Keep canonical integration cases.
  - [ ] Avoid overfitting to parser internals.
  - [ ] Prefer diagnostic codes in failure fixtures.

### Phase-end audit / style review / validation

- [ ] Run the full style guide checklist against all touched code.
- [ ] Run:
  - [ ] `cargo fmt`
  - [ ] `cargo clippy`
  - [ ] `cargo test`
  - [ ] `cargo run -- tests`
  - [ ] `just validate`
- [ ] Manually review frontend stage boundaries:
  - [ ] Tokenizer only creates tokens and reserves the keyword.
  - [ ] Header parsing owns `export` surface parsing and facade metadata.
  - [ ] Dependency sorting only sorts declaration headers and dependency edges.
  - [ ] AST consumes header-built visibility and validates semantic type surfaces where necessary.
  - [ ] HIR remains unaffected except through existing AST outputs.
  - [ ] Backends remain unaffected by facade syntax.
- [ ] Confirm all changed docs match implemented behavior.

## Suggested implementation order for a coding agent

1. Complete Phase 0 and list current fixtures to update.
2. Implement Phases 1 and 2 together if the context budget allows; they are mechanical and foundational.
3. Implement Phase 3 in one chunk, with parser tests.
4. Implement Phase 4 in one chunk, with facade import/re-export tests.
5. Implement Phase 5 separately; receiver-method and public type leak validation are the highest-risk semantic pieces.
6. Implement Phase 6 tests after behavior is stable.
7. Implement Phase 7 docs after tests confirm final syntax.
8. Complete Phase 8 cleanup and full validation.

## Definition of done

- `export` is reserved globally.
- `export` is accepted only in `#mod.bst`.
- `#mod.bst` supports private imports and private top-level helper declarations.
- Public `#mod.bst` declarations require `export`.
- `export import @path { Symbol }` works.
- `export @path { Symbol }` works as sugar.
- `export @path` namespace exports are rejected with a clear deferred-feature diagnostic.
- Unmarked facade declarations are not importable from outside the module.
- Plain facade imports are not public.
- Public re-export aliases define public API names.
- Import-only facades work.
- Direct special-file imports remain rejected.
- Public APIs cannot leak private types.
- Receiver methods do not bypass explicit facade exports.
- Documentation and implementation matrix describe the new model.
- Full validation passes.
