# Beanstalk Module/Library and Alpha Hardening Implementation Plan

## Purpose

This plan breaks the next Beanstalk module/library hardening work into stable, agent-sized phases.

Scope:

1. Module and library system hardening
2. External package stabilization
3. Core library behavior tightening
4. Maintenance and refactor audits

Out of scope for this plan:

- Pattern matching hardening
- Choice feature expansion
- Full package manager / remote registry design
- Source-library HIR caching
- Wildcard or namespace imports
- External receiver methods
- Wasm maturity beyond clean unsupported-backend diagnostics

The target size is roughly **10–20 large commits**, where each phase is a stable validation boundary. A phase may be wider or narrower if needed, but each phase must leave the repo in a validated state.

Each phase assumes an LLM coding agent will implement it. Before editing, read:

- `AGENTS.md`
- `docs/codebase-style-guide.md`
- `docs/compiler-design-overview.md`
- `docs/language-overview.md`
- `docs/src/docs/progress/#page.bst`
- `tests/cases/manifest.toml`

---

## Current correction note

Phase 3 was partially implemented, but it duplicated the older `#root_folders` config behavior by adding `#library_folders` beside it. That is design drift.

The correction is:

- `#library_folders` replaces `#root_folders`.
- `#root_folders` should be removed from active config behavior and rejected with a migration diagnostic.
- `#libraries` should also point users to `#library_folders`.
- Configured library folders are **scan roots**, not import prefixes.
- Each direct child directory of a configured library folder becomes an import prefix.
- `/lib` remains the default scan root when `#library_folders` is omitted.
- Source libraries and regular modules share the same module visibility model.
- `#mod.bst` is a **module facade**, not a separate “library facade” concept.

Example:

```text
project/
  lib/
    helper/#mod.bst
  packages/
    parser/#mod.bst
```

with:

```beanstalk
#library_folders = { @lib, @packages }
```

means these import prefixes exist:

```beanstalk
import @helper/thing
import @parser/thing
```

It does **not** mean `@lib/helper` or `@packages/parser` are import prefixes.

---

## Phase 0 — Planning scaffold and roadmap index

### Status

Implemented.

### Validation boundary

- Roadmap plan exists in `docs/roadmap/plans/module-library-alpha-hardening-plan.md`.
- Matrix and roadmap should continue to be updated as behavior changes.

---

## Phase 1 — Document final module visibility semantics

### Status

Implemented, but review again during Phase 3 reconciliation.

### Current follow-up checks

- [ ] Ensure docs use “module facade” rather than “library facade”.
- [ ] Ensure docs explain that configured library folders are scan roots.
- [ ] Ensure docs do not describe source libraries as a separate visibility system.
- [ ] Ensure docs do not describe `#root_folders` as active config behavior.

---

## Phase 2 — Module root and `#mod.bst` structural validation

### Status

Mostly implemented.

### Current follow-up checks

- [ ] Confirm every `#mod.bst`, not only project-local source-library facades, receives `FileRole::ModuleFacade`.
- [ ] Replace user-facing “Library facade files (#mod.bst)” diagnostics with “Module facade files (#mod.bst)”.
- [ ] Replace comments that describe `#mod.bst` as a library-only facade.
- [ ] Keep the structural rules:
  - private top-level declarations rejected
  - top-level runtime statements rejected
  - runtime templates rejected
  - `#import` rejected outside `#mod.bst`
  - `#mod.bst` does not create an implicit `start`

### Validation

- [ ] Run `just validate`.

---

# Phase 3 correction sequence

The old Phase 3 should be replaced by the following refactor sequence before continuing to Phase 4.

---

## Phase 3A — Remove `#root_folders` as an active config surface

### Summary

Make `#library_folders` the only active config key for project-local source-library scan roots.

### Why

The current code has both `root_folders` and `library_folders`, which creates two competing import-root concepts. This duplicates behavior and will corrupt later import/path hardening.

### Primary files

- `src/projects/settings.rs`
- `src/build_system/project_config/validation.rs`
- `src/build_system/tests/create_project_modules_tests.rs`
- `tests/cases/manifest.toml`
- `docs/src/docs/progress/#page.bst`
- `docs/language-overview.md`
- `docs/compiler-design-overview.md`

### Implementation steps

- [ ] Remove `Config.root_folders`.
- [ ] Remove `root_folders` initialization from `Config::new` and `Default`.
- [ ] Remove `#root_folders` from the list of standard config keys.
- [ ] Add a hard config diagnostic for `#root_folders`.
- [ ] Update the existing `#libraries` deprecated-key diagnostic to point to `#library_folders`, not `#root_folders`.
- [ ] Remove tests that assert `root_folders` are parsed successfully.
- [ ] Add/keep migration tests:
  - `config_root_folders_replaced_by_library_folders_rejected`
  - `config_libraries_replaced_by_library_folders_rejected`

### Diagnostic direction

```text
Config key '#root_folders' has been replaced. Use '#library_folders' instead.
```

Suggestion:

```text
Rename '#root_folders' to '#library_folders'. Configured library folders are scan roots; each direct child folder becomes an import prefix.
```

### Audit

- [ ] No remaining active code reads `config.root_folders`.
- [ ] No docs describe `#root_folders` as supported.
- [ ] No tests rely on `#root_folders` success behavior.

### Validation

- [ ] Run `just validate`.

---

## Phase 3B — Collapse duplicate config folder parsing

### Summary

Remove the duplicate root-folder parsing path and keep one focused parser for `#library_folders`.

### Why

`parse_root_folders_value`, `parse_library_folders_value`, `validate_root_folder_path`, and `validate_library_folder_path` overlap heavily. The corrected model only needs `#library_folders`.

### Primary files

- `src/build_system/project_config/validation.rs`
- config parser unit tests

### Implementation steps

- [ ] Delete `parse_root_folders_value`.
- [ ] Delete `validate_root_folder_path`.
- [ ] Delete or simplify generic dedupe helpers that only existed for root folders.
- [ ] Keep one `parse_library_folders_value`.
- [ ] Keep one `validate_library_folder_path`.
- [ ] Ensure duplicates are hard errors, not silent dedupe.
- [ ] Ensure validation rejects:
  - empty entries
  - absolute paths
  - `..`
  - nested paths such as `@lib/helpers`
  - path aliases
  - malformed values
- [ ] Ensure values may be written as path tokens, symbols, or strings only if current config syntax intentionally supports them.

### Tests

Add or confirm:

```text
config_library_folders_default_lib_success
config_library_folders_custom_scan_root_success
config_library_folders_multiple_scan_roots_success
config_library_folders_missing_default_ignored
config_library_folders_missing_explicit_rejected
config_library_folders_absolute_path_rejected
config_library_folders_dotdot_rejected
config_library_folders_nested_path_rejected
config_library_folders_duplicate_rejected
```

### Audit

- [ ] No parser function still says `root_folders`.
- [ ] No duplicate folder-list parser remains.
- [ ] Error wording consistently says `#library_folders`.

### Validation

- [ ] Run `just validate`.

---

## Phase 3C — Remove root-folder path resolution

### Summary

Remove the old project-root-folder import branch from `ProjectPathResolver`.

### Why

Under the corrected model, configured library folders are scan roots. Their direct children become import prefixes. The configured folder names themselves are not import roots.

### Primary files

- `src/compiler_frontend/paths/path_resolution.rs`
- `src/build_system/create_project_modules/module_discovery.rs`
- resolver tests

### Implementation steps

- [ ] Remove `CompileTimePathBase::ProjectRootFolder`, unless path literals still need it for a separate non-import feature. Do not keep it for imports.
- [ ] Remove `root_folder_names`.
- [ ] Remove `collect_root_folder_names`.
- [ ] Remove `extract_root_folder_name`.
- [ ] Remove `matches_root_folder`.
- [ ] Remove `validate_entry_root_collisions`.
- [ ] Change `ProjectPathResolver::new` to no longer accept `root_folders`.
- [ ] Update `build_project_path_resolver` to call the simplified constructor.
- [ ] Update path-base resolution so import resolution checks:
  1. relative/same-file forms if currently supported
  2. source library prefixes
  3. entry-root fallback, until Phase 4 tightens path rules further

### Tests

Add or confirm:

```text
library_scan_root_name_is_not_import_prefix
library_direct_child_is_import_prefix
entry_root_import_still_works_until_phase_4
source_library_prefix_wins_consistently
```

### Audit

- [ ] No user-facing diagnostic mentions configured `#root_folders`.
- [ ] No root-folder collision logic remains.
- [ ] Resolver comments describe scan roots and source-library prefixes accurately.

### Validation

- [ ] Run `just validate`.

---

## Phase 3D — Reframe source-library collision validation

### Summary

Replace old `#root_folders` entry-root collision logic with source-library-prefix collision rules.

### Why

After removing root folders, the remaining ambiguity is whether an entry-root folder and a source-library prefix share the same first segment.

Example:

```text
src/helper/
lib/helper/
```

If `@helper` resolves to the source library, `src/helper` may become unreachable through the same bare import spelling.

### Recommended policy

Hard error.

### Implementation steps

- [ ] Add validation for collisions between entry-root top-level folders and discovered source-library prefixes.
- [ ] Keep existing collision validation between project-local source libraries and builder-provided source libraries.
- [ ] Keep existing collision validation between configured scan roots that discover the same prefix.
- [ ] Ensure diagnostics explain that library prefixes win or that ambiguity is disallowed. Prefer disallowed.

### Tests

```text
library_prefix_collision_with_builder_library_rejected
library_prefix_collision_across_scan_roots_rejected
library_prefix_collision_with_entry_root_folder_rejected
```

### Validation

- [ ] Run `just validate`.

---

## Phase 3E — Module-facade terminology cleanup

### Summary

Align naming with the corrected semantic model.

### Why

`#mod.bst` is not a library-only concept. It is the module facade for normal modules, source-library modules, and directories with other build-system `#` files.

### Implementation steps

- [ ] Replace user-facing “Library facade files (#mod.bst)” with “Module facade files (#mod.bst)”.
- [ ] Replace comments that describe `FileRole::ModuleFacade` as library-only.
- [ ] Ensure docs consistently say:
  - module facade
  - source-library scan folder
  - source-library prefix
  - project-local source library
- [ ] Ensure docs do not imply that libraries have a separate visibility model.

### Tests

Existing diagnostics tests may need expected message updates.

### Validation

- [ ] Run `just validate`.

---

## Phase 3F — Matrix and docs reconciliation

### Summary

Update docs and matrix after the refactor lands.

### Matrix changes required

In `docs/src/docs/progress/#page.bst`, update these rows:

- Paths and imports
- Source library roots
- Builder-provided source libraries
- Project-local libraries
- Import re-exports
- Deferred or reserved surfaces

Required matrix wording:

```text
#library_folders defines project-local source-library scan folders.
Each direct child directory of a configured scan folder becomes an import prefix.
The configured scan folder name itself is not an import prefix.
#root_folders has been replaced by #library_folders and is rejected with a migration diagnostic.
Source libraries and regular modules share the same module visibility model.
```

Also ensure the matrix says:

- `/lib` is the default scan folder when `#library_folders` is omitted.
- missing default scan folders are ignored.
- explicitly configured missing scan folders are config errors.
- wildcard/namespace imports remain deferred.
- source-library HIR caching remains deferred.
- package manager/versioning/remote registry remain deferred.

### Docs changes required

Update:

- `docs/language-overview.md`
- `docs/compiler-design-overview.md`
- `docs/src/docs/progress/#page.bst`
- this plan

### Validation

- [ ] Run `just validate`.

---

## Stop condition before continuing Phase 4

Do not continue to Phase 4 until all are true:

- [ ] `Config` has no `root_folders`.
- [ ] `#root_folders` is rejected with a clear migration diagnostic.
- [ ] `#libraries` points users to `#library_folders`.
- [ ] `ProjectPathResolver` has no root-folder path branch.
- [ ] `#library_folders` is the only config key for project-local source-library scan roots.
- [ ] Configured scan folder names are not import prefixes.
- [ ] Each direct child of a configured scan folder becomes an import prefix.
- [ ] Docs say configured folders are scan roots, not import prefixes.
- [ ] User-facing `#mod.bst` wording says module facade, not library facade.
- [ ] `just validate` passes.

---

## Phase 4 — Import path restriction and canonicalization hardening

### Summary

Reject unsupported import path forms early and consistently.

### Dependency

Only start after Phase 3A–3F are complete.

### Behavior to implement

- [ ] Reject imports containing `..`.
- [ ] Reject import paths that escape a module root, library root, or project root.
- [ ] Keep path rendering platform-stable.
- [ ] Treat import paths as logically case-sensitive.
- [ ] Ensure external package imports are skipped during filesystem reachable-file discovery.
- [ ] Decide whether `@./child` remains valid or should be normalized/rejected.

### Tests

```text
import_dotdot_rejected
import_relative_parent_rejected
import_escape_project_root_rejected
import_escape_library_root_rejected
import_escape_module_root_rejected
import_sibling_child_success
import_source_library_prefix_success
import_core_package_skips_filesystem_resolution
import_path_separator_normalized_diagnostic
import_case_sensitive_symbol_mismatch_rejected
```

### Validation

- [ ] Run `just validate`.

---

## Phase 5 — `#import` re-export parity with regular imports

### Summary

Make `#import` match regular import syntax for single, grouped, nested grouped, and aliased imports.

### Behavior to implement

- [ ] `#import @path/Symbol` re-exports `Symbol`.
- [ ] `#import @path/Symbol as PublicName` re-exports as `PublicName`.
- [ ] Grouped `#import` is supported.
- [ ] Nested grouped `#import` is supported if regular imports support it.
- [ ] Re-export aliases are public API names.
- [ ] Re-export aliases do not create local bindings in `#mod.bst`.
- [ ] Duplicate public export names are hard errors.

### Validation

- [ ] Run `just validate`.

---

## Phase 6 — Module privacy and submodule boundary enforcement

### Summary

Enforce module-private boundaries consistently across normal modules, source-library modules, and submodules.

### Behavior to implement

- [ ] Files inside the same module can use private declarations according to normal rules.
- [ ] Files outside a module cannot import private implementation files directly.
- [ ] Parent modules access submodule exports through the submodule `#mod.bst`.
- [ ] Sibling modules cannot bypass another sibling module’s `#mod.bst`.
- [ ] A module without `#mod.bst` has no outward public API.
- [ ] `#` inside a module means visible across files in that module, not automatically exported outside the module.

### Validation

- [ ] Run `just validate`.

---

## Phase 7 — External package builder validation

### Summary

Validate external package availability and backend support before lowering.

### Behavior to implement

- [ ] Builder-provided external package registry declares supported packages.
- [ ] Importing an unsupported external package fails immediately.
- [ ] Unsupported package failure happens before backend lowering.
- [ ] JS-supported core packages still work.
- [ ] Wasm/HTML-Wasm unsupported package imports fail with structured diagnostics.

### Validation

- [ ] Run `just validate`.

---

## Phase 8 — External opaque type alias rejection

### Summary

Reject aliases to external opaque types for Alpha.

### Behavior to implement

- [ ] External opaque types remain valid in external function signatures.
- [ ] User type aliases cannot target external opaque types.
- [ ] User code cannot construct external opaque types.
- [ ] User code cannot field-access external opaque types.

### Validation

- [ ] Run `just validate`.

---

## Phase 9 — External `InlineExpression` lowering support

### Summary

Implement JS `InlineExpression` lowering metadata for external package functions.

### Behavior to implement

- [ ] Support pure expression lowering for external functions with inline metadata.
- [ ] Preserve argument order.
- [ ] Emit each argument expression once.
- [ ] Avoid helper emission for inline-only functions.
- [ ] Reject unsupported backends cleanly.

### Validation

- [ ] Run `just validate`.

---

## Phase 10 — External constant policy

### Summary

Document and enforce current external constant limits.

### Behavior to implement

- [ ] External scalar constants continue to work in const contexts.
- [ ] External non-scalar constants are rejected.
- [ ] Add roadmap item for external non-scalar constant design.

### Validation

- [ ] Run `just validate`.

---

## Phase 11 — Core random runtime contract

### Summary

Define and test `@core/random` edge behavior.

### Behavior to implement

- [ ] `random_float()` returns `[0.0, 1.0)`.
- [ ] `random_int(min, max)` is inclusive at both ends.
- [ ] `random_int(min, max)` swaps bounds when `min > max`.
- [ ] `min == max` returns that value.
- [ ] seeded random remains deferred.

### Validation

- [ ] Run `just validate`.

---

## Phase 12 — Core text backend-defined behavior contract

### Summary

Pin current `@core/text` behavior as backend-defined for Alpha.

### Behavior to implement

- [ ] `length(text)` follows backend-defined behavior.
- [ ] `is_empty(text)` is true only for the exactly empty string.
- [ ] empty-needle/prefix/suffix behavior follows backend-defined JS behavior for now.
- [ ] receiver methods remain deferred until method-library imports exist.

### Validation

- [ ] Run `just validate`.

---

## Phase 13 — Import binding visible-name registry audit

### Summary

Audit and refactor import binding so all visible names pass through one collision path.

### Behavior to verify

Every file-local visible name should be registered through one unified collision path:

- same-file declarations
- source imports
- source import aliases
- grouped/nested grouped aliases
- external imports
- external aliases
- type aliases
- prelude symbols
- builtins
- re-export public names

### Validation

- [ ] Run `just validate`.

---

## Phase 14 — Panic/unwrap audit

### Summary

Audit user-input paths for `panic!`, `todo!`, `unimplemented!`, `.unwrap()`, and `.expect()`.

### Areas

- `src/build_system/create_project_modules/`
- `src/build_system/project_config/`
- `src/compiler_frontend/tokenizer/`
- `src/compiler_frontend/headers/`
- `src/compiler_frontend/declaration_syntax/`
- `src/compiler_frontend/ast/`
- touched JS backend external lowering paths

### Validation

- [ ] Run `just validate`.

---

## Phase 15 — Final matrix and roadmap reconciliation

### Summary

Reconcile matrix, roadmap, docs, and manifest after all behavior changes.

### Validation

- [ ] Run `just validate`.

---

## Recommended commit sequence after this correction

1. `config: remove root_folders in favor of library_folders`
2. `config: collapse duplicate library folder parsing`
3. `paths: remove root-folder import resolution`
4. `build: validate source-library prefix collisions`
5. `docs: reconcile module facade and library folder wording`
6. `frontend: harden import path restrictions`
7. `frontend: align hash import reexports with regular imports`
8. `frontend: enforce module privacy across libraries and submodules`
9. `external: validate package support per builder`
10. `frontend: reject aliases to external opaque types`
11. `js: implement external inline expression lowering`
12. `external: document and enforce scalar external constants`
13. `core: define random package edge behavior`
14. `core: pin backend-defined text package behavior`
15. `frontend: unify visible name collision registration`
16. `compiler: audit user-input panic paths`
17. `docs: reconcile progress matrix and roadmap`
