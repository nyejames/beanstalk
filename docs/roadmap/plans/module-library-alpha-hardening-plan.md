# Beanstalk Module/Library and Alpha Hardening Implementation Plan

## Purpose

This plan breaks the next Beanstalk hardening work into stable, agent-sized phases.

Scope:

1. Module and library system hardening
2. External package stabilization
3. Core library behavior tightening
4. Maintenance and refactor audits

Out of scope for this plan:

- Pattern matching hardening
- Choice feature expansion
- External package user-authored binding file design
- Full package manager / remote registry design
- Source-library HIR caching
- Wildcard or namespace imports
- External receiver methods
- Wasm maturity beyond clean unsupported-backend diagnostics

The target size is roughly **10–20 large commits**, where each phase is a stable validation boundary. A phase may be implemented as one commit or split temporarily during work, but the final repository history should aim for one coherent commit per phase where practical.

Each phase assumes an LLM coding agent will implement it. The phase descriptions include concrete files to inspect, expected behavior, test targets, documentation updates, acceptance criteria, and validation steps.

---

## Canonical project references

Before starting any phase, the agent should read the relevant references:

- `AGENTS.md`
- `docs/codebase-style-guide.md`
- `docs/compiler-design-overview.md`
- `docs/language-overview.md`
- `docs/memory-management-design.md`
- `docs/roadmap/roadmap.md`
- `docs/src/docs/progress/#page.bst`
- `tests/cases/manifest.toml`

For this plan, the most important source-of-truth points are:

- Module visibility is defined by `#mod.bst`.
- Modules can have other `#` entry/root files such as `#page.bst`, but `#mod.bst` is the export surface.
- A folder containing one or more `#` files is a module root.
- A module may exist without `#mod.bst` if it is only a build-system entry such as a page, but it exports nothing outside itself.
- If a module exports anything outside itself, it must do so through `#mod.bst`.
- `#mod.bst` is outward-facing API only.
- `#mod.bst` may contain:
  - `#import` re-exports
  - normal public top-level declarations such as exported constants, functions, types, choices, and type aliases
- `#mod.bst` may not contain:
  - private declarations
  - top-level runtime/start code
  - imports or re-exports from outside its own module, except where normal public imports from other modules are explicitly allowed by language rules
- `#import` syntax should match regular import syntax, including grouped imports and per-symbol aliases.
- `#import` re-export aliases are public API names.
- Imports cannot use `..` or project-root escaping.
- Imports can come only from:
  - config-defined library folders
  - core/standard library package roots
  - sibling or child directories inside the current module
- Library folders are config-defined. `/lib` is only the default convention for new projects.
- Libraries and normal modules share the same visibility system. Do not treat source libraries as a separate module concept.
- Submodules are visible only according to module ancestry: a parent module can expose or use submodule APIs according to the `#mod.bst` visibility rules, but outside modules cannot bypass the relevant facade.
- External packages stay mostly Rust-side metadata for now, but `InlineExpression` support should come soon.
- Unsupported external packages should be rejected by builder validation as soon as imported.
- External opaque types should not be allowed in type aliases.
- External receiver methods remain deferred.
- External constants beyond scalar compile-time values need a separate implementation plan.
- `random_int(min, max)` should swap bounds when `min > max`.
- `random_int(min, max)` is inclusive at both ends.
- `random_float()` returns `[0.0, 1.0)`.
- `@core/text` string behavior is backend-defined for now.
- Core text methods are future work after method-library imports exist.

---

## Phase 0 — Planning scaffold and roadmap index

### Summary

Create the roadmap anchor for this hardening work before code changes start. This gives later commits a place to link detailed plans and keeps `docs/roadmap/roadmap.md` useful as the project-level index.

### Why this phase exists

The current roadmap is intentionally light. This plan is broad enough that it should be linked as a named Alpha hardening effort instead of being hidden in commit history.

### Primary files

- `docs/roadmap/roadmap.md`
- `docs/src/docs/progress/#page.bst`

### Implementation steps

- [ ] Add a new roadmap entry: **Module/library and external package Alpha hardening**.
- [ ] Link this plan once it exists in the repo, for example:
  - `docs/roadmap/plans/module-library-alpha-hardening-plan.md`
- [ ] Add a short scope summary:
  - module visibility and `#mod.bst`
  - source library roots
  - external package validation
  - core library runtime contracts
  - import-binding cleanup
- [ ] Explicitly note that pattern matching and choices are intentionally not covered by this plan.
- [ ] In `docs/src/docs/progress/#page.bst`, add or adjust watch-point language saying:
  - source libraries and regular modules share the same visibility model
  - library folders are config-defined, not hardcoded to `/lib`
  - `#mod.bst` is the outward-facing export facade
  - wildcard/namespace imports remain deferred
  - source-library HIR caching remains deferred
  - package manager/versioning/remote registries remain deferred

### Audit / style-guide review

- [ ] Ensure wording does not describe `/lib` as the only supported library folder.
- [ ] Ensure source libraries are not framed as a separate visibility concept from modules.
- [ ] Ensure deferred features are explicit, not vague TODOs.

### Validation

- [ ] Run docs build through `just validate` if the docs page changes.
- [ ] At minimum run the docs build command used by the project if the full validation suite is too heavy during drafting.

### Acceptance criteria

- The roadmap has a clear entry for this hardening pass.
- The implementation matrix accurately describes the intended direction before implementation begins.
- No code behavior changes in this phase.

---

## Phase 1 — Document final module visibility semantics

### Summary

Update language/compiler docs so the module model is clear before tightening implementation behavior.

### Why this phase exists

The important design correction is that **source libraries and regular modules are not separate concepts**. Libraries are just modules discovered through config-defined library roots. Visibility is still module visibility, governed by `#mod.bst`.

If this is not documented clearly, later diagnostics and tests will look arbitrary.

### Primary files

- `docs/language-overview.md`
- `docs/compiler-design-overview.md`
- `docs/src/docs/progress/#page.bst`
- optionally `docs/src/docs/getting-started/#page.bst` if user-facing docs need a shorter intro

### Implementation steps

- [x] Add a clear section to `docs/language-overview.md` explaining:
  - what makes a directory a module root
  - how `#mod.bst` defines exported API
  - how other `#` files such as `#page.bst` can coexist with `#mod.bst`
  - that a module without `#mod.bst` exports nothing
  - that top-level runtime/start code belongs to build-system entry files, not `#mod.bst`
- [x] Clarify that `#mod.bst` may contain:
  - `#import` re-exports
  - exported constants
  - exported functions
  - exported types/choices/type aliases
- [x] Clarify that `#mod.bst` may not contain:
  - private declarations
  - top-level runtime statements
  - runtime templates/start-function code
- [x] Clarify same-module private access:
  - files inside the same module may import/use private implementation files according to normal internal module rules
  - outside modules must import through the module facade
- [x] Clarify submodule visibility:
  - modules can contain submodules
  - submodule exports are visible upward only according to explicit module facade rules
  - outside modules cannot bypass intermediate facades
- [x] Clarify library roots:
  - config-defined library folders are scanned
  - `/lib` is a default convention, not a hardcoded semantic rule
- [x] Clarify import path restrictions:
  - no `..` imports
  - no escape from module/library/project boundaries
  - imports should use config-defined roots, core/standard roots, or sibling/child module structure
- [x] Update `docs/compiler-design-overview.md` Stage 0 language to match the final model:
  - project structure discovers module roots and config-defined library folders
  - source libraries do not have a separate visibility model
  - module export surfaces are derived from `#mod.bst`
- [x] Update `docs/src/docs/progress/#page.bst` rows for:
  - Paths and imports
  - Source library roots
  - Builder-provided source libraries
  - Project-local libraries
  - Import re-exports

### Tests

No compiler behavior tests are required in this phase unless doc examples are compiled by the docs build. If examples are compiled, keep them minimal and valid.

### Audit / style-guide review

- [x] Check that terminology uses **module**, **module root**, **library folder**, and **source library** consistently.
- [x] Remove language implying that source libraries are a distinct semantic system.
- [x] Ensure docs distinguish `#mod.bst` from `#page.bst`.
- [x] Ensure deferred features are marked as deferred explicitly.

### Validation

- [x] Run `just validate`.

### Acceptance criteria

- A reader can understand how module exports work without reading implementation code.
- Docs state that library folders are config-defined.
- Docs state that `/lib` is only a default.
- Docs state that `#mod.bst` is the only outward-facing export surface for a module.

---

## Phase 2 — Module root and `#mod.bst` structural validation

### Summary

Enforce structural rules for `#mod.bst` and module roots.

### Why this phase exists

The module system depends on `#mod.bst` being a disciplined facade. If `#mod.bst` can contain private code or runtime start code, it becomes both a module implementation file and an export surface, which weakens the model.

### Primary files to inspect

- `src/build_system/create_project_modules/`
- `src/build_system/create_project_modules/module_discovery.rs`
- `src/build_system/create_project_modules/reachable_file_discovery.rs`
- `src/build_system/create_project_modules/import_scanning.rs`
- `src/compiler_frontend/headers/`
- `src/compiler_frontend/headers/parse_file_headers.rs`
- `src/compiler_frontend/declaration_syntax/`
- `src/compiler_frontend/compiler_messages/compiler_errors.rs`
- `tests/cases/manifest.toml`

Exact file names may differ. Inspect the current tree before editing.

### Behavior to implement or verify

- [x] A directory with one or more `#*.bst` files is treated as a module root.
- [x] `#mod.bst` is optional for non-exporting module roots such as page-only modules.
- [x] A module that exports outside itself must expose those exports through `#mod.bst`.
- [x] `#mod.bst` allows:
  - `#import` re-exports
  - normal public top-level declarations
- [x] `#mod.bst` rejects:
  - private top-level declarations
  - top-level runtime statements
  - runtime templates/start code
  - body-like code not attached to a public declaration
- [x] `#mod.bst` does not create a start function.
- [x] A non-entry implementation file outside `#mod.bst` may still contain normal top-level declarations according to existing module rules.
- [x] Private declarations inside normal module implementation files remain private to the module.

### Diagnostics to add or improve

- [x] `#mod.bst` contains private declaration.
  - Suggestion: “Add `#` to export it, move it to an implementation file, or remove it from `#mod.bst`.”
- [x] `#mod.bst` contains top-level runtime code.
  - Suggestion: “Move runtime entry code to a build-system entry file such as `#page.bst`.”
- [x] `#import` used outside `#mod.bst`.
  - Suggestion: “Use `import` for local imports, or move the re-export into `#mod.bst`.”
- [x] Multiple invalid items in `#mod.bst` should aggregate diagnostics where practical.

### Integration tests to add

Suggested cases:

```text
mod_file_allows_public_const
mod_file_allows_public_function
mod_file_allows_public_type
mod_file_rejects_private_const
mod_file_rejects_private_function
mod_file_rejects_runtime_statement
mod_file_rejects_runtime_template
mod_file_does_not_create_start
hash_page_without_mod_exports_nothing
hash_page_with_mod_exports_facade
hash_import_outside_mod_rejected
```

### Audit / style-guide review

- [x] No user-input `panic!`, `todo!`, `.unwrap()`, or `.expect()` paths.
- [x] Diagnostics use structured compiler errors with source locations.
- [x] Keep build-system discovery separate from AST semantic validation.
- [x] Do not create duplicate module-root detection logic if an existing discovery helper owns it.

### Validation

- [x] Run `just validate`.

### Acceptance criteria

- `#mod.bst` is structurally constrained as an API facade.
- Invalid `#mod.bst` content fails with clear diagnostics.
- Existing page builds still work.
- Existing library imports still work.

---

## Phase 3 — Config-defined library folders

### Summary

Make library discovery follow config-defined library folders instead of treating `/lib` as a fixed semantic root.

### Why this phase exists

The intended design is that `/lib` is the default for new projects, but library folders come from config. Hardcoding `/lib` makes the system less general and makes docs misleading.

### Primary files to inspect

- `src/projects/settings.rs`
- `src/build_system/project_config/`
- `src/build_system/project_config/parsing.rs`
- `src/build_system/create_project_modules/module_discovery.rs`
- `src/build_system/create_project_modules/reachable_file_discovery.rs`
- `src/libraries/`
- `tests/cases/manifest.toml`

### Behavior to implement or verify

- [ ] `#config.bst` can define the library folders used by project structure discovery.
- [ ] `/lib` remains the default for new projects if no custom config value is provided.
- [ ] Every configured library folder is scanned.
- [ ] Missing optional library folders should either:
  - be ignored if they are defaults, or
  - fail if explicitly configured.
- [ ] Decide and document this behavior inside the implementation comments and docs.
- [ ] Library folder entries must be normalized and validated.
- [ ] Absolute paths are rejected unless the existing project config design intentionally allows them.
- [ ] Nested path entries are rejected or constrained according to existing config root-folder rules.
- [ ] Duplicate library folder entries are rejected or deduplicated with a warning. Prefer hard error for clarity.
- [ ] Prefix collisions between configured library roots and builder-provided libraries remain hard errors.
- [ ] Builder-provided source libraries still work.

### Diagnostics to add or improve

- [ ] Configured library folder does not exist.
- [ ] Configured library folder path is absolute.
- [ ] Configured library folder path contains `..`.
- [ ] Configured library folder collides with another configured library folder.
- [ ] Configured library folder collides with builder-provided source library.
- [ ] Source library root missing `#mod.bst` when it intends to expose exports.

### Integration tests to add

Suggested cases:

```text
config_library_folder_default_lib_success
config_custom_library_folder_success
config_multiple_library_folders_success
config_library_folder_missing_rejected
config_library_folder_absolute_path_rejected
config_library_folder_dotdot_rejected
config_library_folder_duplicate_rejected
config_library_folder_builder_collision_rejected
custom_library_import_resolves_through_mod
```

### Audit / style-guide review

- [ ] Do not introduce a second config parser path for library folders.
- [ ] Prefer extending the existing config map/settings flow.
- [ ] Keep file I/O in build-system/project-structure code, not frontend AST.
- [ ] Add concise WHAT/WHY comments where default `/lib` fallback is selected.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Library discovery uses config-defined library folders.
- `/lib` remains the default convention.
- Tests cover custom and default library folder behavior.
- Docs and matrix no longer imply `/lib` is semantically special.

---

## Phase 4 — Import path restriction and canonicalization hardening

### Summary

Reject unsupported import path forms early and consistently.

### Why this phase exists

The intended design now rejects `..` in imports entirely and encourages shared behavior to live in config-defined libraries. Import paths should form a clean branch-like structure inside modules, plus explicit configured roots.

### Primary files to inspect

- `src/compiler_frontend/tokenizer/`
- `src/compiler_frontend/headers/`
- `src/compiler_frontend/declaration_syntax/`
- `src/build_system/create_project_modules/import_scanning.rs`
- `src/build_system/create_project_modules/reachable_file_discovery.rs`
- path resolver modules under `src/build_system/`, `src/projects/`, or `src/libraries/`
- `src/compiler_frontend/compiler_messages/compiler_errors.rs`

### Behavior to implement or verify

- [ ] Reject imports containing `..`.
- [ ] Reject import paths that escape a module root, library root, or project root.
- [ ] Imports may target:
  - config-defined library folders
  - core/standard package roots
  - sibling/child paths inside the same module branch
- [ ] Relative parent imports are not allowed.
- [ ] Decide whether `@./child` remains valid or should be normalized to child-only paths.
  - Recommended: allow `@./child` only if already supported and useful; reject `@../child`.
- [ ] Normalize path separators internally.
- [ ] Render diagnostics with stable slash-separated paths.
- [ ] Treat import paths as case-sensitive logically, regardless of filesystem.
- [ ] Ensure external package imports are skipped during filesystem reachable-file discovery.
- [ ] Ensure path normalization happens before privacy/collision checks where necessary, but do not accept paths that should be rejected.

### Diagnostics to add or improve

- [ ] Import path contains `..`.
  - Suggestion: “Move shared code into a configured library folder and import it through the module facade.”
- [ ] Import path escapes module root.
- [ ] Import path targets a file outside allowed roots.
- [ ] Import path uses unsupported path separator or malformed segment.
- [ ] Import path differs only by case from a discovered module/symbol, if such detection is practical.
  - This can be deferred if expensive.

### Integration tests to add

Suggested cases:

```text
import_dotdot_rejected
import_relative_parent_rejected
import_escape_project_root_rejected
import_escape_library_root_rejected
import_escape_module_root_rejected
import_sibling_child_success
import_config_library_root_success
import_core_package_skips_filesystem_resolution
import_path_separator_normalized_diagnostic
import_case_sensitive_symbol_mismatch_rejected
```

### Audit / style-guide review

- [ ] Avoid duplicating import path parsing between header import scanning and full import binding.
- [ ] If duplication exists, create a focused stage-appropriate helper with a clear owner.
- [ ] Keep syntax/token diagnostics distinct from filesystem resolution diagnostics.
- [ ] No panics on malformed import paths.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- `..` imports are rejected everywhere.
- Import path behavior is platform-stable.
- External package imports do not trigger filesystem lookups.
- Tests cover direct import resolution and reachable-file discovery.

---

## Phase 5 — `#import` re-export parity with regular imports

### Summary

Make `#import` match regular import syntax for single, grouped, nested grouped, and aliased imports.

### Why this phase exists

`#import` should be conceptually simple: it re-exports symbols from inside the module through the module facade. It should not introduce a separate syntax or partial parser.

### Primary files to inspect

- `src/compiler_frontend/headers/`
- `src/compiler_frontend/declaration_syntax/`
- `src/compiler_frontend/ast/`
- import binding modules under AST
- module symbol/export map structures
- `tests/cases/manifest.toml`

### Behavior to implement or verify

- [ ] `#import @path/Symbol` re-exports `Symbol`.
- [ ] `#import @path/Symbol as PublicName` re-exports as `PublicName`.
- [ ] Grouped `#import` is supported:
  - `#import @path { A, B as C }`
- [ ] Nested grouped `#import` is supported if regular imports support it.
- [ ] `#import` aliases define public API names.
- [ ] Re-export aliases do not create local bindings in `#mod.bst`.
- [ ] Re-export aliases do not create local bindings in sibling implementation files.
- [ ] `#import` can re-export source declarations from inside its own module.
- [ ] `#import` cannot re-export private implementation files from outside its module.
- [ ] `#import` can re-export public symbols imported from another module/library facade if normal module rules allow that.
- [ ] `#import` can re-export supported external package symbols if external re-exports remain intended.
- [ ] Duplicate public export names are hard errors.
- [ ] Alias case-convention warnings should match regular import behavior where applicable.

### Diagnostics to add or improve

- [ ] `#import` target is outside current module and not a public importable symbol.
- [ ] `#import` alias collides with an existing public export.
- [ ] `#import` alias collides with a public declaration in `#mod.bst`.
- [ ] `#import` grouped syntax has invalid alias.
- [ ] `#import` creates no local binding; using the alias inside `#mod.bst` should produce a normal unresolved-name diagnostic.

### Integration tests to add

Suggested cases:

```text
mod_reexport_single_symbol_success
mod_reexport_single_alias_success
mod_reexport_grouped_success
mod_reexport_grouped_alias_success
mod_reexport_nested_grouped_alias_success
mod_reexport_alias_is_public_name
mod_reexport_alias_not_local_binding
mod_reexport_duplicate_public_name_rejected
mod_reexport_private_outside_module_rejected
mod_reexport_external_function_success
mod_reexport_external_type_rejected_or_deferred
mod_reexport_external_constant_success
mod_reexport_group_level_alias_rejected
```

Adjust external type expectations based on Phase 8.

### Audit / style-guide review

- [ ] Reuse regular import parsing/alias expansion where possible.
- [ ] Do not create a parallel grouped import parser for `#import`.
- [ ] Keep public export-map construction separate from file-local visible-name binding.
- [ ] Comments should explain why re-exports intentionally do not create local bindings.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- `#import` has parity with regular imports for supported syntax.
- Re-export aliases are public API names.
- Re-exports do not create local bindings.
- Duplicate exports fail clearly.

---

## Phase 6 — Module privacy and submodule boundary enforcement

### Summary

Enforce module-private boundaries consistently across normal modules, libraries, and submodules.

### Why this phase exists

The central rule is: **all projects, libraries, and submodules follow the same module visibility rules**. If this is not enforced now, library behavior will drift away from normal project modules.

### Primary files to inspect

- `src/build_system/create_project_modules/module_discovery.rs`
- `src/build_system/create_project_modules/reachable_file_discovery.rs`
- `src/compiler_frontend/headers/`
- `src/compiler_frontend/ast/`
- AST import binding / module symbol visibility code
- module path/scope structures

### Behavior to implement or verify

- [ ] Files inside the same module can use internal/private declarations according to normal rules.
- [ ] Files outside a module cannot import private implementation files directly.
- [ ] A parent module may access submodule exports through the submodule `#mod.bst`.
- [ ] A sibling module may not bypass another sibling module’s `#mod.bst`.
- [ ] A child module may not automatically access parent private implementation details unless explicitly allowed by existing module rules.
- [ ] If current behavior allows child-to-parent private access, decide whether to keep or reject it.
  - Recommended for simplicity: no implicit private access across module roots.
- [ ] A module with no `#mod.bst` has no outward public API.
- [ ] Direct file imports across a module boundary are rejected even if the target declaration is marked `#` inside an implementation file.
- [ ] `#` inside a module means visible across files in that module, not automatically exported outside the module unless exposed by `#mod.bst`.

### Diagnostics to add or improve

- [ ] Import crosses module boundary without using `#mod.bst`.
- [ ] Import targets private implementation file from outside module.
- [ ] Import targets a public `#` declaration that was not exposed by the module facade.
- [ ] Module has no public facade.
  - Suggestion: “Add `#mod.bst` and re-export the symbol.”

### Integration tests to add

Suggested cases:

```text
same_module_private_import_success
cross_module_private_import_rejected
cross_module_hash_decl_not_exported_without_mod_rejected
cross_module_mod_reexport_success
submodule_facade_import_success
submodule_private_bypass_rejected
sibling_module_private_bypass_rejected
module_without_mod_exports_nothing
library_and_regular_module_visibility_match
```

### Audit / style-guide review

- [ ] Ensure no special-case source-library privacy model remains.
- [ ] Keep ancestry/module-root checks in one focused place.
- [ ] Ensure diagnostics point at the import path, not a vague module-level location.
- [ ] Verify tests use realistic multi-file fixtures.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Module privacy is consistent for project modules and source libraries.
- `#` is internal module visibility unless surfaced by `#mod.bst`.
- Direct private file imports across module boundaries are rejected.

---

## Phase 7 — External package builder validation

### Summary

Validate external package availability and backend support before imports can proceed into lowering.

### Why this phase exists

Unsupported external packages should fail early and clearly. The preferred policy is builder validation / import-time rejection, not deep backend failure.

### Primary files to inspect

- `src/build_system/build.rs`
- `src/projects/`
- `src/libraries/`
- `src/compiler_frontend/external_packages/` or equivalent
- backend builder implementations
- JS/HTML builder external package registry code
- HTML-Wasm builder code
- `tests/cases/manifest.toml`

### Behavior to implement or verify

- [ ] Builder-provided external package registry declares which packages are available for that builder.
- [ ] Importing an unsupported external package fails immediately.
- [ ] Unsupported package failure should happen through builder/config/frontend validation before backend lowering.
- [ ] `@core/math`, `@core/text`, `@core/random`, and `@core/time` support is explicit per builder.
- [ ] HTML/JS supports current core packages.
- [ ] Wasm or HTML-Wasm unsupported package imports fail with structured diagnostics unless support already exists.
- [ ] Prelude package symbols such as `io` and `IO` remain builder-provided.
- [ ] Explicit imports cannot shadow prelude names.

### Diagnostics to add or improve

- [ ] External package not supported by selected builder.
- [ ] External symbol not found in supported package.
- [ ] External package exists but symbol is backend-unsupported.
- [ ] External import shadows prelude symbol.

### Integration tests to add

Suggested cases:

```text
external_package_unsupported_builder_math_rejected
external_package_unsupported_builder_text_rejected
external_package_unsupported_builder_random_rejected
external_package_unsupported_builder_time_rejected
external_package_missing_symbol_rejected
external_package_import_rejects_prelude_shadow
external_package_import_rejects_unsupported_backend_before_lowering
```

### Audit / style-guide review

- [ ] Keep builder capability validation outside AST expression parsing.
- [ ] Do not make AST globally search external packages without file-local visibility.
- [ ] Ensure diagnostics preserve interned paths/source locations.
- [ ] Avoid duplicating package availability checks in multiple backends.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Unsupported package imports fail before backend lowering.
- JS-supported packages still work.
- Wasm/HTML-Wasm unsupported behavior is explicit and tested.

---

## Phase 8 — External type alias rejection and opaque type policy

### Summary

Reject type aliases to external opaque types for Alpha.

### Why this phase exists

External opaque types can be passed/returned by external functions, but allowing transparent aliases to them blurs the type model. The intended answer for now is **no external opaque type aliases**.

### Primary files to inspect

- `src/compiler_frontend/ast/`
- type alias resolution code
- external package type resolution code
- `src/compiler_frontend/type_coercion/`
- `docs/language-overview.md`
- `docs/src/docs/progress/#page.bst`

### Behavior to implement or verify

- [ ] External opaque types remain valid in external package function signatures.
- [ ] User code cannot construct external opaque types.
- [ ] User code cannot field-access external opaque types.
- [ ] User type aliases cannot target external opaque types.
- [ ] Existing aliases to source types, structs, choices, options, collections, and builtins continue to work.
- [ ] Aliases to external scalar-like built-in types should be rejected unless they are true Beanstalk types.

### Diagnostics to add or improve

- [ ] Type alias targets external opaque type.
  - Suggestion: “Use the external type directly in supported external function signatures; user aliases for opaque external types are not supported.”
- [ ] External opaque type constructor call attempted.
- [ ] External opaque type field access attempted.

### Integration tests to add

Suggested cases:

```text
external_opaque_type_alias_rejected
external_opaque_type_constructor_rejected
external_opaque_type_field_access_rejected
external_opaque_type_function_param_success
external_opaque_type_function_return_success
```

Only add success cases if the current package registry has a practical opaque type fixture. Otherwise add a small test-only external package registry fixture if the test infrastructure supports it.

### Matrix/docs updates

- [ ] Update external platform packages row:
  - external opaque types are pass/return-only
  - aliases to opaque external types are rejected for Alpha
- [ ] Update type aliases row:
  - remove or correct any claim that external package type aliases are supported
- [ ] Add deferred note:
  - external opaque type aliases require a separate type identity/ABI design

### Audit / style-guide review

- [ ] Ensure alias rejection happens during type alias resolution, not late backend lowering.
- [ ] Keep source aliases and external opaque aliases distinguished clearly in diagnostics.
- [ ] Do not break aliases to imported source types.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- External opaque type aliases are rejected.
- External opaque types still work where currently supported by external functions.
- Matrix no longer claims unsupported alias behavior.

---

## Phase 9 — External `InlineExpression` lowering support

### Summary

Implement JS `InlineExpression` lowering metadata for external package functions.

### Why this phase exists

`InlineExpression` support is expected soon. This is a small but important external package capability, especially for simple package functions that should lower directly into JS expressions instead of helper calls.

### Primary files to inspect

- external package metadata definitions
- JS backend external call lowering
- JS expression emitter
- JS helper registration/emission
- core package registry definitions
- tests for JS backend artifact assertions

### Behavior to implement

- [ ] Define or locate the `InlineExpression` metadata shape.
- [ ] Support pure expression lowering for external functions where metadata provides a JS expression template or lowering callback.
- [ ] Ensure argument order is preserved.
- [ ] Ensure argument expressions are emitted exactly once.
- [ ] Ensure lowering respects result type.
- [ ] Ensure `InlineExpression` functions do not emit unused JS helpers.
- [ ] Ensure unsupported backends reject `InlineExpression` if no equivalent lowering exists.
- [ ] Add at least one core package function using `InlineExpression` if appropriate.
  - Candidate: `@core/math` simple functions if they currently lower through helper wrappers.
  - Keep this conservative.

### Diagnostics to add or improve

- [ ] Inline expression metadata references unsupported ABI type.
- [ ] Inline expression metadata arity mismatch.
- [ ] Inline expression lowering requested in unsupported backend.
- [ ] Inline expression argument lowering produces statements/prelude where pure expression is required.

### Integration/backend tests to add

Suggested cases:

```text
external_inline_expression_js_shape
external_inline_expression_no_helper_emitted
external_inline_expression_argument_order
external_inline_expression_nested_call
external_inline_expression_unsupported_backend_rejected
```

### Audit / style-guide review

- [ ] Do not add stringly JS concatenation in scattered call sites.
- [ ] Centralize inline external lowering inside JS external-call lowering.
- [ ] Add WHAT/WHY comments explaining purity/evaluation-order assumptions.
- [ ] Do not expand external receiver methods in this phase.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- `InlineExpression` metadata has real JS lowering support.
- Generated JS is stable and tested.
- Unsupported backends reject cleanly.
- No external receiver method work is introduced.

---

## Phase 10 — External constant policy and future implementation-plan marker

### Summary

Document and enforce current external constant limits, while explicitly creating a future plan item for non-scalar external constants.

### Why this phase exists

External constants currently support scalar compile-time values. The desired future is broader, but that needs its own implementation plan. For now, make the current policy honest and enforced.

### Primary files to inspect

- external package constant metadata
- AST constant folding / expression parsing
- `docs/src/docs/progress/#page.bst`
- `docs/roadmap/roadmap.md`

### Behavior to implement or verify

- [ ] External scalar constants continue to work in const contexts.
- [ ] External non-scalar constants are rejected unless already fully designed.
- [ ] External constants lower as ordinary literals where supported.
- [ ] External constants require explicit package import unless prelude-provided.
- [ ] External constant aliases through regular imports and `#import` re-exports behave consistently.

### Diagnostics to add or improve

- [ ] External non-scalar constant unsupported.
  - Suggestion: “Non-scalar external constants require a dedicated ABI/lowering design.”
- [ ] External constant used without import.
- [ ] External constant alias collides with visible name.

### Integration tests to add

Suggested cases:

```text
external_scalar_constant_const_context_success
external_scalar_constant_alias_success
external_scalar_constant_reexport_success
external_constant_non_imported_rejected
external_non_scalar_constant_rejected
```

### Roadmap/matrix updates

- [ ] Add roadmap item:
  - **External non-scalar constant design**
- [ ] Matrix should say:
  - scalar compile-time external constants supported
  - non-scalar external constants deferred to separate plan

### Audit / style-guide review

- [ ] Keep constant folding behavior frontend-owned.
- [ ] Do not leak backend runtime values into compile-time constant evaluation.
- [ ] Keep package metadata validation close to package registry construction.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Current external constant support is enforced and documented.
- Non-scalar external constants are explicitly deferred.
- There is a roadmap entry for the future design.

---

## Phase 11 — Core random runtime contract

### Summary

Define and test `@core/random` edge behavior.

### Why this phase exists

Small standard library edge cases should not remain undefined. `random_int(min, max)` should be inclusive and should swap bounds when `min > max`.

### Primary files to inspect

- core random package registry
- JS backend external package lowering
- JS runtime helper emission
- tests for core random package
- `docs/language-overview.md`
- `docs/src/docs/progress/#page.bst`

### Behavior to implement

- [ ] `random_float()` returns a Float in `[0.0, 1.0)`.
- [ ] `random_int(min, max)` returns an Int.
- [ ] `random_int(min, max)` is inclusive at both ends.
- [ ] If `min > max`, bounds are swapped.
- [ ] If `min == max`, that exact value is returned.
- [ ] Negative ranges are supported.
- [ ] Non-Int args are rejected by existing type checking.
- [ ] Seeded random remains deferred.

### Diagnostics to add or improve

- [ ] Arity errors are clear.
- [ ] Type errors are clear.
- [ ] Unsupported backend errors are clear from Phase 7.

### Integration/backend tests to add

Suggested cases:

```text
core_random_float_range_shape
core_random_int_equal_bounds
core_random_int_swaps_bounds
core_random_int_negative_range
core_random_int_inclusive_bounds_smoke
core_random_int_type_error
core_random_seeded_random_deferred_or_absent
```

Because randomness is nondeterministic, avoid brittle exact-output tests except for `min == max`.

### Docs/matrix updates

- [ ] Update `@core/random` row:
  - inclusive range
  - swapped bounds
  - seeded random deferred

### Audit / style-guide review

- [ ] Keep JS helper implementation simple and documented.
- [ ] Do not introduce seeded randomness.
- [ ] Avoid brittle random tests.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Random integer edge behavior is defined and tested.
- No major random API expansion occurs.
- Matrix is accurate.

---

## Phase 12 — Core text backend-defined behavior contract

### Summary

Pin current `@core/text` behavior as backend-defined for Alpha and add edge tests.

### Why this phase exists

String behavior can become a design sink quickly. For Alpha, the correct move is to document that text behavior is backend-defined and test JS behavior without pretending to solve Unicode semantics.

### Primary files to inspect

- core text package registry
- JS backend external package lowering
- JS runtime helper emission
- text package tests
- `docs/language-overview.md`
- `docs/src/docs/progress/#page.bst`

### Behavior to implement or verify

- [ ] `length(text)` follows backend-defined behavior.
  - For JS, this likely means JS string `.length`.
- [ ] `is_empty(text)` is true only when the string is exactly empty.
- [ ] `contains(text, "")` follows backend-defined JS behavior unless explicitly wrapped.
- [ ] `starts_with(text, "")` follows backend-defined JS behavior unless explicitly wrapped.
- [ ] `ends_with(text, "")` follows backend-defined JS behavior unless explicitly wrapped.
- [ ] Current free-function API remains.
- [ ] Receiver-method API is deferred until method-library imports exist.

### Diagnostics to add or improve

- [ ] Type errors for non-string args.
- [ ] Arity errors for text functions.
- [ ] Unsupported backend errors from Phase 7.

### Integration/backend tests to add

Suggested cases:

```text
core_text_length_ascii
core_text_length_unicode_backend_defined_js
core_text_is_empty_exact_empty
core_text_is_empty_non_empty
core_text_contains_empty_needle_backend_defined
core_text_starts_with_empty_prefix_backend_defined
core_text_ends_with_empty_suffix_backend_defined
core_text_type_errors
core_text_receiver_methods_deferred_or_absent
```

### Docs/matrix updates

- [ ] Update `@core/text` row:
  - JS-backed behavior is backend-defined for Alpha
  - receiver methods deferred until method-library imports are designed

### Audit / style-guide review

- [ ] Avoid overpromising Unicode behavior.
- [ ] Do not introduce receiver methods in this phase.
- [ ] Keep tests descriptive about backend-defined behavior.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Text edge behavior is documented and tested.
- Unicode behavior is not accidentally specified beyond current backend behavior.
- Receiver-method expansion remains deferred.

---

## Phase 13 — Import binding unified visible-name registry audit

### Summary

Audit and refactor import binding so all visible names pass through one collision path.

### Why this phase exists

The docs describe a unified user-visible name registry for collision checks. This is the right design. The risk is implementation drift across source imports, external imports, aliases, type aliases, prelude symbols, builtins, and same-file declarations.

### Primary files to inspect

- AST import binding code
- module symbol tables
- source declaration visibility maps
- external symbol visibility maps
- type alias visibility maps
- builtin/prelude registration code
- import alias diagnostics
- grouped import expansion code

### Behavior to implement or verify

Every file-local visible name should be registered through one unified collision path:

- [ ] same-file declarations
- [ ] source imports
- [ ] source import aliases
- [ ] grouped source import aliases
- [ ] nested grouped source import aliases
- [ ] external function imports
- [ ] external constant imports
- [ ] external type imports, if still supported directly
- [ ] external aliases
- [ ] type aliases
- [ ] prelude symbols
- [ ] builtins

### Refactor steps

- [ ] Identify current visible-name registration helpers.
- [ ] Identify duplicate collision checks.
- [ ] Pick one owner for collision registration.
- [ ] Move shared behavior into that owner.
- [ ] Keep source and external resolution distinct after the name has been accepted.
- [ ] Preserve existing diagnostic specificity.
- [ ] Delete stale wrappers or parallel paths once the unified path is threaded through.
- [ ] Add comments explaining the invariant:
  - “Every user-visible spelling in a file must pass through this registry before being inserted into a visibility map.”

### Tests to add or strengthen

Suggested cases:

```text
alias_collision_source_and_external
alias_collision_external_and_type_alias
alias_collision_grouped_import_and_builtin
alias_collision_prelude_and_source_alias
alias_collision_same_file_decl_and_grouped_alias
alias_collision_reexport_public_name
alias_case_mismatch_warning_source
alias_case_mismatch_warning_external
```

### Audit / style-guide review

- [ ] Check for duplicated validation logic left behind.
- [ ] Check for stage-boundary leaks.
- [ ] Avoid a broad generic “utils” module unless clearly justified.
- [ ] Ensure the new abstraction name is more readable than the duplicated code.
- [ ] Do not preserve old APIs through forwarding shims.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- One clear collision registration path exists.
- Similar alias/collision cases produce consistent diagnostics.
- No duplicated import-collision logic remains without explicit justification.

---

## Phase 14 — Panic/unwrap audit for frontend and project structure

### Summary

Audit user-input paths for panics, `todo!`, `unimplemented!`, `.unwrap()`, and `.expect()`.

### Why this phase exists

The style guide is clear: active frontend stages must reject unsupported or malformed user input with structured diagnostics. This audit should happen after module/import changes, because those changes touch many user-input paths.

### Primary areas to inspect

- `src/build_system/create_project_modules/`
- `src/build_system/project_config/`
- `src/compiler_frontend/tokenizer/`
- `src/compiler_frontend/headers/`
- `src/compiler_frontend/declaration_syntax/`
- `src/compiler_frontend/ast/`
- `src/compiler_frontend/type_coercion/`
- `src/compiler_frontend/hir/`
- `src/compiler_frontend/analysis/borrow_checker/`
- JS backend external lowering paths touched by this plan

### Audit categories

For each panic/unwrap/todo/expect found:

| Category | Action |
|---|---|
| Proven internal invariant | Keep, but add or improve comment |
| User-input reachable | Replace with structured diagnostic |
| Test-only | Keep isolated under test-only code |
| Dead path | Delete |
| Deferred feature stub | Replace with structured “deferred/unsupported” diagnostic if reachable |

### Implementation steps

- [ ] Search for `panic!`.
- [ ] Search for `todo!`.
- [ ] Search for `unimplemented!`.
- [ ] Search for `.unwrap()`.
- [ ] Search for `.expect(`.
- [ ] Classify each occurrence in touched modules.
- [ ] Fix user-input reachable cases.
- [ ] Add targeted regression tests for fixed cases.
- [ ] Add comments for retained internal invariants.

### Tests to add

Tests depend on findings. Likely categories:

```text
diagnostic_malformed_import_no_panic
diagnostic_invalid_mod_file_no_panic
diagnostic_external_package_unsupported_no_panic
diagnostic_invalid_library_config_no_panic
```

### Audit / style-guide review

- [ ] Ensure retained panics are genuinely compiler bugs.
- [ ] Ensure retained `unwrap`/`expect` calls have nearby justification when non-obvious.
- [ ] Ensure replacement diagnostics use correct error categories.
- [ ] Remove dead code rather than documenting it.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- No known user-input path in touched areas can trigger panic/todo/unwrap.
- Retained invariant panics are justified.
- New diagnostics have regression coverage.

---

## Phase 15 — Matrix and roadmap final reconciliation

### Summary

Reconcile the implementation matrix and roadmap after all behavior changes.

### Why this phase exists

This plan changes several current matrix claims. The final phase ensures the docs are honest after implementation, not only before it.

### Primary files

- `docs/src/docs/progress/#page.bst`
- `docs/roadmap/roadmap.md`
- `docs/language-overview.md`
- `docs/compiler-design-overview.md`
- `tests/cases/manifest.toml`

### Matrix updates to verify

- [ ] Paths and imports:
  - no `..`
  - config-defined library roots
  - same-module sibling/child imports
  - no wildcard/namespace imports
- [ ] Source library roots:
  - config-defined folders
  - same module visibility model as regular modules
  - `#mod.bst` facade
- [ ] Builder-provided source libraries:
  - same module visibility rules
  - collision behavior
- [ ] Project-local libraries:
  - `/lib` is default, not semantic hardcode
- [ ] Import re-exports:
  - single/grouped/nested/aliased `#import`
  - aliases are public names
  - no local binding created
- [ ] External platform packages:
  - unsupported builder import rejection
  - external opaque type alias rejection
  - external receiver methods deferred
  - non-scalar external constants deferred
  - `InlineExpression` supported if Phase 9 completed
- [ ] Core random:
  - inclusive int range
  - swapped bounds
  - seeded random deferred
- [ ] Core text:
  - backend-defined behavior
  - receiver methods deferred
- [ ] Deferred/reserved surfaces:
  - source-library HIR caching
  - package manager/versioning/remote registry
  - namespace/wildcard imports
  - external receiver methods
  - external non-scalar constants

### Roadmap updates

- [ ] Add or update high-level roadmap items:
  - External non-scalar constant design
  - Method library imports / external receiver method design
  - Package manager/versioning/remote registry
  - Source-library HIR caching
- [ ] Remove or revise roadmap items that this plan completes.
- [ ] Keep pattern matching hardening separate.

### Test manifest audit

- [ ] Ensure new test cases are listed in `tests/cases/manifest.toml`.
- [ ] Tag cases consistently:
  - `imports`
  - `libraries`
  - `diagnostics`
  - `external-packages`
  - `js-backend`
  - `config`
- [ ] Avoid duplicate cases proving the same behavior.

### Audit / style-guide review

- [ ] The matrix should describe current implementation, not aspirational behavior.
- [ ] Deferred features should be named explicitly.
- [ ] No stale claims remain about external opaque type aliases.
- [ ] No stale claims remain about `/lib` being the only library folder.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Roadmap and matrix match implemented behavior.
- Deferred features are explicit.
- Test manifest contains all new cases.
- Docs and implementation do not contradict each other.

---

## Final deferred feature list

These should be explicitly represented in the matrix and/or roadmap as deferred.

### Module/import deferred features

- Wildcard imports
- Namespace imports
- Source-library HIR caching
- Package manager
- Remote registry
- Version resolution
- Lockfiles
- Parent-directory imports with `..`

### External package deferred features

- User-authored external binding files
- External receiver methods
- External opaque type aliases
- External non-scalar constants
- External package Wasm lowering beyond explicit supported subsets

### Core library deferred features

- Seeded random
- Full Unicode text semantics
- Text receiver methods until method-library imports exist

### Backend deferred features

- Wasm support for core packages unless explicitly implemented
- HTML-Wasm Alpha support
- Wasm maturity beyond clean diagnostics

---

## Recommended commit sequence

This is the ideal 10–20 commit path:

1. `docs: add module/library hardening roadmap scaffold`
2. `docs: clarify module visibility and configured library folders`
3. `frontend: validate mod facade structure`
4. `build: discover configured library folders`
5. `frontend: reject unsupported import path escapes`
6. `frontend: align hash import reexports with regular imports`
7. `frontend: enforce module privacy across libraries and submodules`
8. `external: validate package support per builder`
9. `frontend: reject aliases to external opaque types`
10. `js: implement external inline expression lowering`
11. `external: document and enforce scalar external constants`
12. `core: define random package edge behavior`
13. `core: pin backend-defined text package behavior`
14. `frontend: unify visible name collision registration`
15. `compiler: audit user-input panic paths`
16. `docs: reconcile progress matrix and roadmap`

If Phase 9 grows too large, split it:

- `external: add inline expression metadata validation`
- `js: lower inline external expressions`
- `tests: cover inline external expression JS output`

If Phase 13 grows too large, split it:

- `frontend: introduce unified visible name registry`
- `frontend: thread imports through visible name registry`
- `tests: cover alias collision parity`

---

## General agent instructions for every phase

Before editing:

- [ ] Read `AGENTS.md`.
- [ ] Read the relevant docs named in the phase.
- [ ] Search for existing implementation paths before adding new helpers.
- [ ] Identify similar logic and decide whether to consolidate or keep local.
- [ ] Avoid compatibility wrappers and duplicate legacy paths.
- [ ] Prefer structured diagnostics with source locations.
- [ ] Prefer integration tests for language-visible behavior.
- [ ] Update `tests/cases/manifest.toml` for every new integration case.
- [ ] Update `docs/src/docs/progress/#page.bst` when behavior changes.
- [ ] Run `just validate`.

Each phase should end with an explicit mini-review:

```text
Audit checklist:
- No user-input panics introduced.
- No duplicated validation/lowering logic without justification.
- Stage boundaries remain clean.
- Diagnostics are structured and source-located.
- Matrix/roadmap/docs are accurate for changed behavior.
- `just validate` passes.
```

---

## Highest-value first implementation target

Start with:

> Phase 2 — Module root and `#mod.bst` structural validation

Reason:

- It protects the core API-surface invariant.
- It is smaller than full path/module privacy hardening.
- It gives immediate useful diagnostics.
- It clarifies behavior for later library-folder and re-export work.
- It is a good agent-sized validation boundary.

Then do:

1. Phase 3 — Config-defined library folders
2. Phase 4 — Import path restriction
3. Phase 5 — `#import` re-export parity
4. Phase 6 — Module privacy/submodule enforcement

Those five phases form the true module/library hardening core.
