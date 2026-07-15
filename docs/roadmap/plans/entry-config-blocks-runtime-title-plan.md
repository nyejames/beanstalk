# Entry-local `config:` blocks and runtime title implementation plan

## Active context capsule

ACTIVE_PLAN:
- `docs/roadmap/plans/entry-config-blocks-runtime-title-plan.md`

CURRENT_SLICE:
- Phase: Phase 0 — refresh post-prerequisite context and establish the implementation baseline
- Checklist item: confirm the New Module and Export System plan is complete, remap all code owners on `templates-refactor`, inventory current HTML metadata usage and run baseline validation
- Goal: start from the accepted post-module-system architecture and install a reloadable plan without preserving stale pre-refactor paths
- Non-goals:
  - Do not implement `config:` syntax before the prerequisite module/export work is accepted
  - Do not change `config.bst` behavior during Phase 0
  - Do not migrate HTML page metadata or add `io.set_title` during Phase 0
  - Do not treat the current filenames and symbols in this plan as permanent if the prerequisite work moved their ownership

LAST_GOOD_COMMIT:
- `none` until the first implementation slice is accepted locally

CURRENT_WORKTREE_STATE:
- Clean / known changes: unknown, run `git status --short` before the first slice and refresh this field
- Branch: target branch is `templates-refactor`, verify locally with `git branch --show-current`
- Dedicated worker worktrees: none known, record every worker worktree path before use

RELEVANT_DOCS_THIS_SLICE:
- `AGENTS.md` if present in the local checkout
- `docs/codebase-style-guide.md`
- `docs/compiler-design-overview.md`
- `docs/language-overview.md`
- `docs/roadmap/plans/hash-root-export-block-module-system-plan.md`
- `docs/roadmap/plans/import_values_anonymous_records_plan.md`
- `docs/roadmap/roadmap.md`
- `docs/src/docs/progress/#page.bst`
- `docs/memory-management-design.md` only if implementation unexpectedly changes runtime ownership, borrow or lifetime behavior

RELEVANT_CODE:
- Post-prerequisite module-root header parser: must recognize and isolate one active-root `config:` block without emitting it into normal module declarations or `start`
- Post-prerequisite active/imported root-role model: must apply entry config only for the active module root and suppress it when the same root is imported for public API validation
- Current or post-prerequisite config compilation owner, likely under `src/build_system/project_config/**`: must become the single parser and validator used by `config.bst` and embedded entry config
- Current config-key registry at `src/builder_surface/config_key_registry.rs`: must become one scoped registry instead of parallel project and entry registries
- `src/builder_surface/definition.rs`: builder-declared config key surface
- Module frontend orchestration and backend handoff, currently `src/build_system/create_project_modules/frontend_orchestration.rs` and `src/build_system/build.rs::Module`: carry resolved entry config outside HIR
- HTML page metadata resolution, currently `src/projects/html_project/page_metadata.rs`: replace reserved HIR constant scanning with resolved entry config consumption
- Shared HTML document shell and JS/Wasm artifact paths: preserve identical initial metadata behavior across HTML-JS and HTML-Wasm
- Core IO external function registry and JS helper emission: add demand-driven `io.set_title`
- Integration fixtures, generated project scaffolding, docs source pages and README examples that still use `page_title`, `page_head` or related reserved constants

ACCEPTANCE_CRITERIA:
- One top-level `config:` block is accepted only in an active module root
- The block is an isolated config compilation unit and cannot see surrounding module imports, declarations or runtime values
- The block follows the same syntax, structural restrictions, import policy, constant folding, duplicate handling, value-shape validation and diagnostics as canonical `config.bst`
- `config.bst` and entry blocks are maintained through one parser and validator path
- One generalized config-key registry supports project-only, entry-only and project-and-entry keys
- Unsupported, unknown, wrong-scope, duplicate, non-folding and wrong-shape entries fail with structured `CompilerDiagnostic` values
- Resolved entry config is carried on the compiled module as builder-facing metadata and is not added to HIR
- Imported module roots never apply their entry config to the importing module
- The HTML builder registers and consumes entry keys `title`, `description`, `lang`, `favicon`, `body_style` and `head`
- Reserved top-level HTML constants such as `page_title` and `page_head` no longer affect output and receive targeted migration diagnostics in HTML entry roots
- HTML-JS and HTML-Wasm use the same compile-time entry config when rendering the initial document shell
- A non-empty HTML entry config counts as builder-relevant root activity so a config-only HTML root is not incorrectly treated as API-only
- `io.set_title(StringContent)` is implemented for HTML-JS and updates the live document title
- Reachable `io.set_title` calls are rejected before lowering on HTML-Wasm and other unsupported targets
- No silent runtime no-op is used for unsupported hosts
- Repository examples, scaffolding, tests, user docs, compiler docs, roadmap and progress matrix are updated
- Deliberately deferred features are listed explicitly in the roadmap and progress matrix
- No duplicate config parser, compatibility metadata scanner or stale HIR name-based page metadata path remains
- Every accepted phase passes its focused tests and `just validate`, or records accepted unrelated failures in this capsule

DECISIONS_ALREADY_MADE:
- decision: Entry settings use a top-level `config:` block with normal config constant declarations
  - reason: this matches the declarative shape of `config.bst` and avoids fake namespace assignment such as `html.title #= ...`
  - source/user/date: user design discussion, 2026-07-10
- decision: Entry `config:` uses rules identical to `config.bst`
  - reason: one maintained parser and validator is simpler and prevents two config languages from drifting
  - source/user/date: user design interview answer 1, 2026-07-10
- decision: The block is isolated from the surrounding entry module
  - reason: it behaves as an embedded config file with its own imports and support declarations, avoiding ordering and visibility coupling with `start` code
  - source/user/date: user design interview answer 2, 2026-07-10
- decision: Project and entry keys use one generalized registry with per-key scope metadata
  - reason: parsing, shape checks, diagnostics and builder registration remain unified while wrong-scope keys are rejected
  - source/user/date: user design interview answer 3, 2026-07-10
- decision: V1 includes runtime `io.set_title`
  - reason: title mutation has a reasonable cross-backend host/window meaning while most HTML metadata does not
  - source/user/date: user design interview answer 4, 2026-07-10
- decision: An active module root may contain at most one `config:` block and each key may appear once
  - reason: deterministic config has no merge order or last-write-wins semantics
  - source/user/date: user design interview answer 5, 2026-07-10
- decision: This plan continues on `templates-refactor` after the New Module and Export System plan is complete
  - reason: root roles, `export:` parsing, `config.bst`, source-tree discovery and module handoff may all change first
  - source/user/final framing: 2026-07-10
- decision: Current repository paths are navigation hints, not frozen implementation contracts
  - reason: Phase 0 must adopt the accepted post-prerequisite owners instead of rebuilding around stale pre-refactor code
  - source/user/final framing: 2026-07-10

BLOCKERS / RISKS:
- The New Module and Export System plan is a hard prerequisite. Do not start implementation while its root roles, `config.bst` migration, export block or module handoff remain incomplete
- The `#Import` values and anonymous records plan may later extend config parsing. This plan must leave one reusable config compiler and registry for it rather than a conflicting abstraction
- Current HTML pages often derive `page_head` from surrounding imports. Isolation means those imports must move inside the block and must satisfy the same import policy as `config.bst`
- If current documentation metadata depends on project-local or relative imports that canonical `config.bst` rejects, do not silently broaden only the entry block. Move the dependency to an allowed core/builder source-backed package or obtain a separate design decision to change both config surfaces together
- Captured block tokens and resolved entry config locations must survive worker-local string-table merge/remap correctly
- Imported module roots may contain their own `config:` block. It must be ignored by the importer without suppressing it when that module is compiled as the active root
- Entry config must not accidentally create normal module symbols, exports, HIR constants or runtime statements
- Config-only HTML roots must interact correctly with post-prerequisite module root activity and artifact filtering
- `io.set_title` is host-capability-sensitive. The implementation must not assume every JavaScript target has a browser `document`
- Large docs-site migration may expose hidden dependence on the old reserved constant scanner. Inventory before changing behavior
- Compiling an isolated config unit for every configured root can add frontend cost. Modules without `config:` must stay on a near-zero-overhead path and no new source-tree scan may be introduced

VALIDATION_STATE:
- last command: none for this plan
- result: not run
- known unrelated failures: unknown

DOCS_IMPACT:
- progress matrix needed: yes
- other docs stale:
  - `docs/language-overview.md`
  - `docs/compiler-design-overview.md`
  - `docs/src/docs/project-structure/#page.bst`
  - HTML page/document metadata documentation
  - `docs/src/docs/packages/core/io/#page.bst`
  - generated HTML project scaffolding
  - `README.md`
  - docs source pages using legacy `page_*` constants
  - `docs/roadmap/roadmap.md`
  - `docs/roadmap/plans/import_values_anonymous_records_plan.md` if its config anchors or assumptions need refreshing
- authorized docs updates:
  - yes, update language/compiler docs, docs site, examples, scaffolding, roadmap and progress matrix in this plan
  - do not update memory-management documentation unless implementation changes ownership, borrow or lifetime semantics

NEXT_ACTION:
- In the local `templates-refactor` checkout, confirm the New Module and Export System plan is complete, record the actual commit/worktree state, refresh all code anchors, inventory legacy page metadata dependencies and run Phase 0 validation before changing compiler behavior

---

## Plan status, order and durability

This plan targets the `templates-refactor` branch.

It is intentionally implemented **after**:

1. `docs/roadmap/plans/final-tir-completion-plan.md`
2. `docs/roadmap/plans/hash-root-export-block-module-system-plan.md`

The second item is a hard prerequisite. At minimum, the accepted repository must already provide:

- canonical `config.bst`
- generic hash-root module files
- `ActiveModuleRoot`, `ImportedModuleRoot` and normal-file behavior, or the accepted equivalent
- one strict `export:` block
- source-tree/module-root discovery without duplicate expensive scans
- explicit root activity or equivalent builder artifact metadata
- a compiled `Module` handoff that can be extended without rescanning HIR

The roadmap should place this plan directly after the New Module and Export System plan and before the `#Import` values and anonymous records plan. The numeric plan can remain independently ordered if desired, but this plan must complete before `#Import` work refactors the same config registry and config compilation path.

If `#Import` or anonymous-record work has already landed when implementation begins, Phase 0 must reconcile this plan with that accepted shape. Reuse its config value types and parser owners. Do not create a second path.

This document is deliberately anchored to **semantic owners and stage contracts**, not a frozen source snapshot. Every accepted slice must refresh the active context capsule. Before compaction, update:

- current phase and checklist item
- branch and last good commit
- exact current code paths and symbols
- accepted decisions
- blockers
- validation state
- next action

---

## Current state and post-prerequisite refresh contract

### Informational branch snapshot at plan authoring

On `templates-refactor` before the prerequisite module plan is implemented:

- project config parsing runs a dedicated tokenizer → headers → dependency sort → AST path
- config value extraction and application are coupled to global project `Config`
- the config-key registry is named and documented as project-config-specific
- compiled modules do not yet carry entry config
- the HTML builder scans HIR module constants for reserved names such as `page_title` and `page_head`
- Core IO has demand-driven console and input functions but no title setter

These facts explain the refactor target. They are not promises that the same files or symbols still exist when Phase 0 begins.

### Mandatory Phase 0 remap

Before implementation, locate and record the accepted owners for:

1. module-root source loading and active/imported root classification
2. top-level block parsing and token remapping
3. canonical `config.bst` compilation
4. config-key registration and lookup
5. config structural diagnostics
6. config AST constant extraction and value-shape validation
7. compiled module assembly and string-ID remapping
8. module root activity and HTML artifact filtering
9. HTML document metadata resolution
10. Core IO external function registration
11. JavaScript runtime helper selection/emission
12. backend support validation
13. integration test manifests and docs generation

If any owner moved, update this plan's capsule before coding. Do not add forwarding shims to preserve obsolete paths.

---

## Final agreed design

### Source syntax

```beanstalk
config:
    import @html {default_head}

    title #= "Docs"
    description #= "Documentation pages"
    head #= default_head
;
```

The block body is parsed exactly as an authored `config.bst` source unit.

That means its allowed and rejected syntax follows the canonical config implementation at the time this plan is executed. The initial expected rules are:

Allowed:

- imports allowed by the canonical config import policy
- type aliases
- structs
- choices
- known `#` config-key declarations
- folded string/template values
- folded scalar values and collections supported by registered key shapes
- references to earlier config keys where canonical config allows them
- imported compile-time constants and support types where canonical config allows them

Rejected:

- plain runtime bindings
- mutable bindings
- functions
- runtime or host calls
- runtime statements
- traits
- trait conformances and incompatibility metadata
- standalone page fragments
- nested `config:` blocks
- unknown authored constants
- any import source rejected by canonical `config.bst`

There is no entry-block-only exception. If a rule changes for one config surface, it must change through the shared implementation and be evaluated for both scopes.

### Isolation

An entry `config:` block is an embedded config compilation unit, not an ordinary lexical scope.

It has:

- its own import environment
- its own support declarations
- its own config-key declarations
- the source location of the containing root file for diagnostics

It does not see:

- imports outside the block
- surrounding root-file constants, types, functions or aliases
- module-local implementation files
- runtime/start bindings
- values exported by the surrounding module merely because they are visible to normal module code

The surrounding module does not see:

- imports declared inside `config:`
- support declarations declared inside `config:`
- config-key declarations as ordinary constants
- any config-only generated symbols

### Placement and cardinality

- `config:` is valid only at a top-level item boundary in an active module root
- one active root may contain zero or one `config:` block
- a second block is a structured duplicate-block diagnostic
- `config:` is rejected in normal source files
- `config:` is rejected inside `export:`
- `config:` is rejected inside another block or executable body
- `config:` is not valid inside canonical `config.bst`, which is already a config source unit
- an imported module root's config block is recognized as root-only metadata and is not applied to the importer
- when that same module is compiled as the active root, its block is compiled and passed to the builder

`config` should be implemented consistently with the accepted keyword policy after the module/export plan. Preferred V1 policy is a reserved keyword that is valid only in this top-level form, matching the strict treatment of `export`. If the post-prerequisite parser uses contextual top-level block keywords instead, use that one established mechanism and document the choice.

### Key registration and scope

Use one registry:

```rust
pub enum ConfigKeyScope {
    Project,
    Entry,
    ProjectAndEntry,
}
```

The exact enum name may follow post-prerequisite naming, but the model must remain explicit. Do not use loose `is_project` / `is_entry` booleans.

Each key definition retains or replaces the existing concepts of:

- source-level name
- owner, such as core or backend
- accepted folded value shape
- valid config scope
- any closed-domain values
- stable lookup identity if the post-prerequisite registry has introduced IDs

Required registry behavior:

- look up a key for a specific config scope
- distinguish unknown keys from known-but-wrong-scope keys
- reject conflicting duplicate registration
- reject overlapping registrations whose owner or value shape disagrees
- allow a deliberately shared key through `ProjectAndEntry`
- expose deterministic iteration for diagnostics and tests
- keep builder registration declarative

Project and entry values remain separate config surfaces. A key being valid in both does **not** imply automatic inheritance, overriding or merging. The builder must define any future relationship explicitly. V1 HTML page keys are entry-only.

### Shared config compilation

Generalize the current config pipeline around a source-independent input:

```rust
pub enum ConfigSource {
    ProjectFile {
        path: PathBuf,
    },
    EntryBlock {
        source_file: SourceFileId,
        tokens: ConfigBlockTokens,
        location: SourceLocation,
    },
}
```

This is illustrative, not a frozen API. The accepted implementation must provide the same capabilities:

- file-backed project config
- token/span-backed embedded entry config
- original source locations
- one structural validator
- one import/source-set policy
- one dependency sorting path
- one AST constant-folding path
- one config entry extractor
- one value-shape validator

A recommended shared result is:

```rust
pub struct ResolvedConfigValues {
    pub entries: Vec<ResolvedConfigEntry>,
}

pub struct ResolvedConfigEntry {
    pub key: String,
    pub value: ResolvedConfigValue,
    pub location: SourceLocation,
}

pub enum ResolvedConfigValue {
    String(String),
    Int(i32),
    Bool(bool),
    StringCollection(Vec<String>),
}
```

Use post-prerequisite types if equivalent types already exist. Requirements matter more than names:

- values are already folded and shape-validated
- source order is deterministic
- lookup is clear
- locations remain available for builder diagnostics
- all interned IDs and source locations participate in module remapping
- the type is builder-facing metadata, not HIR

Project config consumes the shared result and applies core fields or backend project settings.

Entry config stores the shared result on the compiled module for builder consumption.

### Compiler and builder ownership

| Concern | Owner |
|---|---|
| Recognize and isolate `config:` from root source | Top-level header/root parser |
| Enforce one block and placement rules | Header/root parser |
| Compile the block using config syntax | Shared config compilation owner |
| Resolve allowed config imports | Shared config compilation owner using the canonical config import policy |
| Fold constants/templates | AST through the shared config path |
| Reject unknown, duplicate, wrong-scope and wrong-shape keys | Shared config validation plus scoped registry |
| Carry resolved entry values | Compiled `Module` side metadata |
| Decide HTML meaning of keys | HTML builder |
| Render initial document metadata | Shared HTML document shell path |
| Mutate title at runtime | Core IO semantic function plus backend lowering |
| Carry config in HIR | Nobody, explicitly forbidden |
| Re-scan HIR constants by name | Nobody, explicitly removed |

### HTML entry keys in V1

The HTML builder registers these entry-only keys:

| Key | Shape | Initial HTML effect |
|---|---|---|
| `title` | String | `<title>` content before project prefix/postfix policy |
| `description` | String | `<meta name="description">` |
| `lang` | String | `<html lang="...">` |
| `favicon` | String | `<link rel="icon">` |
| `body_style` | String | `<body style="...">` |
| `head` | String | raw/folded extra HTML inserted into `<head>` under the existing raw-head policy |

V1 deliberately keeps `head` as one folded string/template value. Authors can compose multiple fragments in an ordinary compile-time template. This plan does not add `+=`, per-key merge strategies or typed head nodes.

Existing document defaults remain unchanged unless the old behavior is intentionally documented otherwise:

- route/project title fallback
- title prefix/postfix
- project favicon fallback
- default language
- default body style
- core CSS and import map ordering
- HTML escaping for title, description, favicon, language and body style
- existing raw insertion policy for extra head HTML

### Builder artifact relevance

A module with a non-empty entry config is not API-only from the HTML builder's perspective.

The post-prerequisite root activity model must allow the HTML builder to consider:

- root/start body activity
- const page fragments
- runtime page fragments
- non-empty HTML entry config

A root containing only:

```beanstalk
config:
    title #= "Empty page"
;
```

must still produce an HTML document unless the accepted HTML project policy explicitly forbids empty bodies. Do not rely on an HIR scan to detect this.

### Legacy HTML metadata constants

Remove the behavior of:

- `page_title`
- `page_description`
- `page_lang`
- `page_favicon`
- `page_body_style`
- `page_head`

These names must not remain a compatibility path.

For HTML active roots, provide targeted migration diagnostics such as:

```text
`page_title` no longer configures the generated page.
Move it into the entry config block:

config:
    title #= ...
;
```

Scope the migration diagnostic narrowly enough that ordinary constants with these names in unrelated package files do not become globally reserved without reason.

After migration:

- HTML metadata resolution does not inspect `HirModule::module_constants`
- duplicate and wrong-type handling comes from shared config validation
- page-metadata-specific diagnostic variants are deleted if no other behavior uses them
- legacy names remain only in migration tests and migration documentation

### Runtime title contract

V1 adds:

```beanstalk
io.set_title("Loading...")
io.set_title([: Score: [score]])
```

Semantic signature:

```text
io.set_title(StringContent) -> Void
```

Runtime behavior:

- HTML-JS sets the live document title
- the helper is emitted only when reachable
- the value uses the same canonical string-content conversion used by other Core IO string boundaries
- the operation is not reactive by itself
- calling it after initial page load overrides the initial title emitted from entry config
- unsupported hosts must not silently ignore the operation

Target support:

| Target | V1 status |
|---|---|
| HTML-JS | Implemented |
| HTML-Wasm | Deliberately deferred, reachable calls rejected before lowering |
| Standalone/non-browser JS | Reject through target capability validation where possible, otherwise fail clearly at runtime rather than no-op |
| Native/window backends | Deliberately deferred until a window/host contract exists |

This plan does not add `io.title(...)`, `io.get_title()`, a window handle or runtime setters for HTML-only metadata.

---

## Diagnostics contract

All source-facing failures use structured `CompilerDiagnostic` values with stable codes and real source locations.

| Case | Expected diagnostic owner |
|---|---|
| Missing `:` after `config` | Header/root syntax diagnostic |
| Unterminated block or missing final `;` | Header/root syntax diagnostic |
| Second `config:` block | Header/root duplicate-block diagnostic with prior location where supported |
| `config:` in normal file | Root-role/config-block placement diagnostic |
| `config:` inside `export:` | Export/config block placement diagnostic |
| Nested `config:` | Config structural diagnostic |
| Runtime binding or statement inside block | Shared invalid-config diagnostic |
| Function, trait or conformance inside block | Shared invalid-config diagnostic |
| Mutable config declaration | Shared invalid-config diagnostic |
| Duplicate key | Shared invalid-config diagnostic |
| Unknown key | Shared invalid-config diagnostic |
| Key known but not valid for entry scope | Shared wrong-config-scope diagnostic |
| Value cannot fold | Shared invalid-config diagnostic |
| Folded value has wrong shape | Shared invalid-config diagnostic |
| Outer module name referenced from isolated block | Normal unresolved-name/import diagnostic in the config compilation unit |
| Import source rejected by canonical config policy | Shared config import diagnostic |
| Legacy `page_*` HTML constant | HTML metadata migration diagnostic |
| Reachable `io.set_title` on unsupported backend | Existing unsupported external function/backend feature diagnostic |

Do not:

- manufacture fake file paths for entry block diagnostics
- route user mistakes through `CompilerError`
- downgrade unknown or wrong-scope keys into silent builder settings
- emit duplicate diagnostics from both config compilation and HTML metadata resolution
- retain a later builder error for a shape that the registry can reject earlier

---

## Performance and incremental-build requirements

Entry config is small, but the implementation must not regress module-heavy builds.

Requirements:

- modules without `config:` do not invoke the embedded config AST pipeline
- the active root is not fully retokenized solely to compile its config block if the post-prerequisite token stream can be safely reused
- config block discovery does not add a source-tree scan
- config-only imports are discovered through the shared config compiler, not added as normal module imports
- imported config support files do not leak into the module's normal visibility environment
- each active module's entry config is compiled at most once per frontend build
- worker-local token/StringId remapping remains deterministic
- no global lock or new custom thread pool is added
- incremental caching of entry config is deferred, but the representation must not prevent a later source-hash cache

Baseline and final checks should include:

```bash
just validate
just bench-frontend-check
just bench-check
just bench-report
```

Use the nearest current commands if the benchmark interface changes. Compare at least:

- project with many modules and no entry config
- project with many small config blocks
- documentation-site build
- module import fanout where imported roots contain config blocks

If a measurable regression appears, profile before accepting the phase.

---

## Testing strategy

Prefer integration tests for language and artifact behavior. Keep unit tests for parser state, registry invariants, value extraction and remapping facts that integration output cannot inspect directly.

### Positive coverage

- one active root with `title`
- all six HTML keys
- folded template as `head`
- block-local allowed import
- block-local struct, choice and type alias support matching `config.bst`
- reference to an earlier config key where canonical config allows it
- two modules with different entry config values
- imported module root config does not affect importer
- config-only HTML root emits a document
- HTML-JS and HTML-Wasm initial shells agree
- route/project fallback still works when no block exists
- project config behavior is unchanged
- key registered for `ProjectAndEntry` works in both surfaces
- `io.set_title` emits only the required JS helper
- runtime title accepts string literals, owned strings and templates

### Negative coverage

- duplicate blocks
- duplicate keys
- unknown key
- project-only key in entry block
- entry-only key in `config.bst`
- wrong shape
- non-folding value
- plain binding
- mutable binding
- function
- runtime call
- trait and conformance
- standalone top-level page fragment
- nested block
- block inside `export:`
- block in normal source file
- surrounding import not visible inside isolated block
- surrounding constant not visible inside isolated block
- relative or project-local import rejected when canonical config rejects it
- legacy `page_*` constants
- reachable `io.set_title` rejected on HTML-Wasm
- unsupported standalone JS host behavior is not a silent no-op

### Regression and stage-boundary coverage

- existing `config.bst` fixtures retain behavior and diagnostic codes unless intentionally generalized
- duplicate project config diagnostics remain stable
- module string-table merge/remap preserves entry config locations
- entry config is absent from HIR and HIR validation needs no config-specific node
- imported roots do not produce entry config in the importing module
- root activity/artifact filtering sees non-empty entry config
- HTML metadata defaults and escaping remain unchanged
- docs build succeeds after repository-wide migration

---

# Phased implementation checklist

## Phase 0 — refresh post-prerequisite context and establish the baseline

### Context

This phase prevents the plan from being implemented against the obsolete pre-module-system shape. It records the exact accepted owners after the New Module and Export System work and identifies migration blockers before code changes.

### Checklist

- [ ] Confirm local repository and branch:
  - [ ] `git status --short`
  - [ ] `git branch --show-current`
  - [ ] `git rev-parse HEAD`
  - [ ] branch is `templates-refactor`
- [ ] Confirm prerequisite completion:
  - [ ] final TIR plan is accepted
  - [ ] hash-root/export-block module plan is accepted
  - [ ] `config.bst` is canonical
  - [ ] active/imported module root roles are implemented
  - [ ] one strict `export:` block is implemented
  - [ ] module root activity and artifact filtering exist
  - [ ] no duplicate Stage 0 root/config scan remains
- [ ] Read local instructions and required docs:
  - [ ] `AGENTS.md` if present
  - [ ] `docs/codebase-style-guide.md`
  - [ ] `docs/compiler-design-overview.md`
  - [ ] `docs/language-overview.md`
  - [ ] prerequisite module plan
  - [ ] `#Import`/anonymous-record plan
  - [ ] progress matrix and roadmap
- [ ] Refresh the `RELEVANT_CODE` section with exact post-prerequisite paths and symbols
- [ ] Inventory the config system:
  - [ ] parser entry point
  - [ ] source-set/import discovery
  - [ ] structural validation
  - [ ] AST folding/extraction
  - [ ] value-shape validation
  - [ ] key registry
  - [ ] project `Config` application
- [ ] Inventory root parser/block machinery introduced for `export:`
- [ ] Inventory module assembly, remapping and root activity
- [ ] Inventory HTML metadata call sites shared by JS and Wasm
- [ ] Inventory Core IO registration, JS helper emission and backend support validation
- [ ] Search and count legacy metadata usage:
  - [ ] `page_title`
  - [ ] `page_description`
  - [ ] `page_lang`
  - [ ] `page_favicon`
  - [ ] `page_body_style`
  - [ ] `page_head`
- [ ] Classify every `page_head` dependency:
  - [ ] allowed core/builder config import
  - [ ] project-local/relative dependency
  - [ ] literal/folded value needing no import
- [ ] If project-local dependencies exist, record the migration strategy or blocker before Phase 6
- [ ] Install this plan in the repository and add it to roadmap order after the module/export plan
- [ ] Run baseline validation:
  - [ ] `just validate`
- [ ] Run baseline performance checks:
  - [ ] `just bench-frontend-check`
  - [ ] `just bench-check`
  - [ ] `just bench-report`
- [ ] Record known unrelated failures and concise benchmark observations
- [ ] Update the active context capsule

### Review / audit / style / validation

- [ ] Confirm this phase changed no language/compiler behavior
- [ ] Confirm no current path was copied into later phases without a semantic owner description
- [ ] Review the plan against `docs/codebase-style-guide.md`
- [ ] Confirm all blockers discovered during inventory are recorded
- [ ] Confirm baseline `just validate` result is recorded
- [ ] Commit only the plan/roadmap installation if that is the accepted slice
- [ ] Set `LAST_GOOD_COMMIT` after acceptance

---

## Phase 1 — generalize the config-key registry with explicit scopes

### Context

The first code slice creates one declarative key registry for project and entry config while preserving all existing `config.bst` behavior. It does not add block syntax.

### Checklist

- [ ] Rename or refactor the project-only registry into one generalized `ConfigKeyRegistry`
- [ ] Add an explicit scope model:
  - [ ] `Project`
  - [ ] `Entry`
  - [ ] `ProjectAndEntry`
- [ ] Preserve key owner and value-shape metadata
- [ ] Add scope-aware lookup APIs
- [ ] Add a diagnostic-friendly distinction between:
  - [ ] unknown key
  - [ ] known key in the wrong scope
- [ ] Add registration helpers that require scope explicitly
- [ ] Add or preserve backend/core registration helpers without boolean-heavy APIs
- [ ] Reject conflicting duplicate registrations as an internal registry construction error
- [ ] Decide and document whether identical duplicate registration is rejected or deduplicated
  - [ ] recommended: reject it as a builder/compiler invariant
- [ ] Update `BuilderSurface` or its accepted successor to expose the generalized registry
- [ ] Update project config callers to request `Project` scope
- [ ] Keep every current project key project-only unless a separate semantic reason exists
- [ ] Add focused registry tests:
  - [ ] project lookup
  - [ ] entry lookup
  - [ ] shared lookup
  - [ ] wrong-scope result
  - [ ] duplicate/conflicting registration
  - [ ] deterministic iteration
- [ ] Ensure no entry config syntax or module field is introduced in this phase

### Review / audit / style / validation

- [ ] Run focused config registry tests
- [ ] Run project config tests
- [ ] Run `just validate`
- [ ] Manual stage-boundary review:
  - [ ] registry remains declarative metadata
  - [ ] no parser logic moved into `BuilderSurface`
  - [ ] user-facing scope failures still use `CompilerDiagnostic`
- [ ] Style review:
  - [ ] descriptive names
  - [ ] explicit enum instead of booleans
  - [ ] no compatibility wrapper retaining two registries
  - [ ] tests outside production files
- [ ] Update active context capsule and `LAST_GOOD_COMMIT`

---

## Phase 2 — extract shared config values, entry extraction and validation

### Context

Current project config validation commonly mixes generic value extraction with global `Config` application. This slice separates shared config semantics from the project-specific consumer without changing accepted source behavior.

### Checklist

- [ ] Introduce a shared resolved config value model
- [ ] Move or expose generic folded value extraction for:
  - [ ] String
  - [ ] Int
  - [ ] Bool
  - [ ] StringCollection
  - [ ] closed string sets
- [ ] Preserve source locations for whole entries and collection elements where diagnostics need them
- [ ] Extract shared duplicate-key validation
- [ ] Extract shared compile-time-constant validation
- [ ] Extract shared shape validation
- [ ] Extract shared scope-aware key validation
- [ ] Keep path-specific project validations, such as package folder rules, in the project config consumer
- [ ] Keep application to typed `Config` fields and project settings in the project config consumer
- [ ] Replace project-specific prose in shared file/module docs
- [ ] Use a named input/context struct for shared validation rather than a long parameter list
- [ ] Keep existing config diagnostics and stable codes where semantics are unchanged
- [ ] Add focused tests for shared value extraction and scope errors
- [ ] Re-run all current project config fixtures to prove behavior preservation
- [ ] Remove obsolete duplicated extraction helpers after callers move

### Review / audit / style / validation

- [ ] Run focused validation/value tests
- [ ] Run project config integration cases
- [ ] Run `just validate`
- [ ] Manual diagnostic audit:
  - [ ] source/config mistakes use `CompilerDiagnostic`
  - [ ] infrastructure failures remain `CompilerError`
  - [ ] no rendered type names or fake paths are stored
- [ ] Style review:
  - [ ] shared module has one clear responsibility
  - [ ] project-only application is not hidden in shared helpers
  - [ ] no clever generic abstraction obscures stage ownership
- [ ] Grep for duplicate value-shape extraction paths
- [ ] Update active context capsule and `LAST_GOOD_COMMIT`

---

## Phase 3 — generalize config compilation around file and embedded sources

### Context

This phase creates the single parser/compiler path required by the design. Canonical `config.bst` remains the only production caller at first. Embedded-source support is added as a tested input shape but not yet connected to root syntax.

### Checklist

- [ ] Refactor the config parsing entry point into a source-independent config compilation service
- [ ] Define a named `ConfigCompilationInput` and `ConfigCompilationResult`
- [ ] Support:
  - [ ] file-backed project config
  - [ ] embedded token/span-backed config source
- [ ] Preserve original source locations for embedded tokens
- [ ] Reuse the canonical config import-root policy for both source kinds
- [ ] Reuse one source-set/import discovery implementation
- [ ] Reuse one tokenizer/header preparation path where applicable
- [ ] For already-tokenized embedded input, avoid retokenizing the entire root file
- [ ] Reuse one dependency sorting path
- [ ] Reuse one AST build/folding path
- [ ] Reuse Phase 2 extraction and validation
- [ ] Ensure imported support files are config-only and do not enter normal module visibility
- [ ] Keep project config stopping before HIR
- [ ] Carry build profile, style directive registry and template const loop limit explicitly
- [ ] Add a config source/context enum for diagnostics and policy, not loose booleans
- [ ] Add unit/pipeline tests that compile equivalent file and embedded config inputs and compare:
  - [ ] resolved values
  - [ ] diagnostics
  - [ ] locations
  - [ ] import behavior
- [ ] Keep production behavior on `config.bst` unchanged
- [ ] Delete the old one-off parser entry once the generalized path is wired

### Review / audit / style / validation

- [ ] Run config parser and pipeline tests
- [ ] Run all project config integration cases
- [ ] Run `just validate`
- [ ] Manual stage-boundary review:
  - [ ] config compilation is build/frontend orchestration, not HIR
  - [ ] AST remains the folding owner
  - [ ] imports are resolved through the config policy, not normal module scope
- [ ] Performance sanity check:
  - [ ] project config is still compiled once
  - [ ] no additional source-tree scan exists
- [ ] Style review against module/file ownership guidance
- [ ] Update active context capsule and `LAST_GOOD_COMMIT`

---

## Phase 4 — add strict top-level `config:` block capture

### Context

This slice adds syntax and root-role behavior but does not yet hand values to the HTML builder. The root parser isolates the block and produces an embedded config source payload for Phase 5.

### Checklist

- [ ] Add `config` to the accepted keyword/contextual-keyword mechanism used by post-prerequisite top-level blocks
- [ ] Recognize `config:` only at a top-level item boundary
- [ ] Parse through the matching top-level closing `;`
- [ ] Capture the block body as an embedded config source payload with:
  - [ ] original source file identity
  - [ ] original token/source locations
  - [ ] block start location
  - [ ] content tokens excluding outer `config:` and final `;`
- [ ] Ensure block tokens are not emitted as:
  - [ ] normal module headers
  - [ ] module constants
  - [ ] exports
  - [ ] `StartFunction` body tokens
  - [ ] page fragments
- [ ] Enforce one block per active root
- [ ] Add root-role behavior:
  - [ ] active root captures one block
  - [ ] imported root recognizes and suppresses its block for the importer
  - [ ] normal file rejects the block
- [ ] Reject `config:` inside `export:`
- [ ] Reject nested `config:`
- [ ] Reject malformed/missing colon and unterminated forms
- [ ] Add block payload remapping for worker-local string tables
- [ ] Thread optional block payload through header aggregation and dependency sorting without making it a declaration graph node
- [ ] Add parser/header tests for every placement and malformed case
- [ ] Add imported-root test proving no payload is applied to importer
- [ ] Do not add HTML key semantics yet

### Review / audit / style / validation

- [ ] Run header/root parser tests
- [ ] Run module import/export tests
- [ ] Run failure fixtures with stable diagnostic codes
- [ ] Run `just validate`
- [ ] Manual stage-boundary review:
  - [ ] header parsing owns block discovery
  - [ ] AST does not rediscover the block from raw source
  - [ ] block does not become a module scope or declaration
  - [ ] imported-root suppression is explicit
- [ ] Grep for accidental `config` handling in runtime statement parsers
- [ ] Style review of parser state and block-matching control flow
- [ ] Update active context capsule and `LAST_GOOD_COMMIT`

---

## Phase 5 — compile isolated entry config and carry it on `Module`

### Context

This phase connects the captured block to the shared config compiler. It proves isolation and adds the builder-facing handoff while keeping entry config outside AST/HIR for the normal module.

### Checklist

- [ ] After file preparation and StringId remapping, detect the active root's optional config payload
- [ ] Invoke the Phase 3 shared config compiler with:
  - [ ] `Entry` scope
  - [ ] the same config import policy as `config.bst`
  - [ ] merged source-backed and binding-backed packages
  - [ ] current style directives
  - [ ] current template const loop limit
  - [ ] original root source location
- [ ] Do not inject surrounding module visibility
- [ ] Do not inject surrounding module constants or types
- [ ] Resolve and validate entries through the generalized registry
- [ ] Add `ResolvedEntryConfig` or the accepted equivalent to compiled `Module`
- [ ] Add empty/default representation for roots without a block
- [ ] Implement StringId/source-location remapping for the module payload
- [ ] Ensure HIR generation consumes only the normal module AST
- [ ] Ensure HIR validation and borrow validation need no config-specific nodes
- [ ] Add module handoff tests
- [ ] Add isolation integration tests:
  - [ ] outer import is invisible
  - [ ] outer constant is invisible
  - [ ] block-local allowed import works
  - [ ] block-local support declarations work exactly as `config.bst`
- [ ] Add key-scope tests:
  - [ ] project-only key rejected in entry block
  - [ ] entry-only key rejected in `config.bst`
  - [ ] shared key accepted in both
- [ ] Update root activity representation so non-empty entry config is available to builder artifact policy
- [ ] Confirm imported roots never contribute entry config to importer
- [ ] Confirm each active module compiles its block at most once

### Review / audit / style / validation

- [ ] Run config, frontend orchestration and module remap tests
- [ ] Run multi-module integration cases
- [ ] Run `just validate`
- [ ] Manual stage-boundary review:
  - [ ] resolved entry config is builder metadata
  - [ ] no entry config in HIR
  - [ ] no normal module symbol leakage
  - [ ] no imported-root metadata leakage
- [ ] Performance check:
  - [ ] no-block modules skip config compilation
  - [ ] no new source-tree scan
  - [ ] root source is not fully retokenized unnecessarily
- [ ] Style review of context structs and module remapping
- [ ] Update active context capsule and `LAST_GOOD_COMMIT`

---

## Phase 6 — migrate HTML compile-time metadata to entry config

### Context

This is the behavior cutover. The HTML builder registers its entry keys, resolves `HtmlPageMetadata` from `Module` entry config and deletes reserved HIR constant scanning.

### Checklist

- [ ] Register HTML entry-only keys:
  - [ ] `title`
  - [ ] `description`
  - [ ] `lang`
  - [ ] `favicon`
  - [ ] `body_style`
  - [ ] `head`
- [ ] Give every key the String value shape
- [ ] Add an HTML entry-config resolver that:
  - [ ] reads already validated resolved values
  - [ ] maps them to `HtmlPageMetadata`
  - [ ] preserves source locations for builder-semantic diagnostics
- [ ] Keep one resolver shared by HTML-JS and HTML-Wasm paths
- [ ] Preserve document shell defaults and escaping behavior
- [ ] Preserve raw `head` insertion behavior
- [ ] Remove HIR module constant scanning from page metadata extraction
- [ ] Remove reserved metadata-name tables and entry-scope prefix matching
- [ ] Remove duplicate/wrong-string page metadata diagnostics now covered by shared config validation
- [ ] Add targeted migration diagnostics for legacy `page_*` constants in HTML active roots
- [ ] Do not keep legacy values as a fallback
- [ ] Update HTML artifact filtering:
  - [ ] non-empty entry config counts as HTML artifact activity
  - [ ] config-only root emits an HTML document
  - [ ] API-only root with no config/body/fragments remains skipped
- [ ] Update HTML compile input/context structs rather than growing long function parameter lists
- [ ] Add artifact tests for every key
- [ ] Add JS/Wasm shell parity tests
- [ ] Add config-only page test
- [ ] Add no-config fallback tests
- [ ] Add legacy migration failure tests
- [ ] Delete or rewrite obsolete page metadata unit tests

### Review / audit / style / validation

- [ ] Run HTML document shell and page metadata tests
- [ ] Run HTML-JS and HTML-Wasm integration cases
- [ ] Run legacy migration diagnostics fixtures
- [ ] Run `just validate`
- [ ] Manual stage-boundary review:
  - [ ] builder interprets metadata
  - [ ] frontend only validates registered shape/scope
  - [ ] no builder rescans HIR constants
  - [ ] JS and Wasm share initial metadata policy
- [ ] Grep audit:
  - [ ] reserved `PAGE_*` scanner constants removed
  - [ ] `extract_html_page_metadata` no longer accepts HIR
  - [ ] old diagnostic variants removed if unused
- [ ] Style review and active context update
- [ ] Set `LAST_GOOD_COMMIT`

---

## Phase 7 — implement runtime `io.set_title` for HTML-JS

### Context

Compile-time entry config sets the initial artifact. Runtime title mutation is a separate host effect and should use the existing external package and demand-driven helper architecture.

### Checklist

- [ ] Add stable external function identity for `IoSetTitle`
- [ ] Add canonical helper/runtime name
- [ ] Register `io.set_title` in Core IO with:
  - [ ] one shared `StringContent` parameter
  - [ ] `Void` success
  - [ ] no source-visible return value
  - [ ] HTML-JS lowering support
  - [ ] no HTML-Wasm lowering
- [ ] Emit the JS helper only when reachable
- [ ] Convert input through canonical Beanstalk string-content conversion
- [ ] Set the live document title
- [ ] Do not silently no-op if browser document support is unavailable
- [ ] Inspect direct/standalone JS lowering:
  - [ ] prefer compile-time target capability rejection
  - [ ] if current architecture cannot distinguish browser JS, emit a clear runtime failure outside a document host
  - [ ] record any broader capability model as deferred
- [ ] Ensure external package validation rejects HTML-Wasm calls before lowering
- [ ] Add frontend call signature tests
- [ ] Add JS helper reachability tests
- [ ] Add generated artifact assertion containing title mutation
- [ ] Add HTML-Wasm rejection fixture with stable diagnostic code
- [ ] Verify runtime title can override initial `config:title`
- [ ] Do not add reactivity, getters or other runtime metadata setters

### Review / audit / style / validation

- [ ] Run Core IO registry tests
- [ ] Run JS backend helper tests
- [ ] Run HTML-JS integration tests
- [ ] Run HTML-Wasm unsupported external function tests
- [ ] Run `just validate`
- [ ] Manual stage-boundary review:
  - [ ] HIR carries only stable external call ID
  - [ ] backend owns host lowering
  - [ ] unsupported target fails before deep lowering
- [ ] Grep helper names and reachability gates for consistency
- [ ] Style review of IO registration tables and helper emission
- [ ] Update active context capsule and `LAST_GOOD_COMMIT`

---

## Phase 8 — migrate repository source, scaffolding and integration fixtures

### Context

The old page metadata spellings must be removed across real Beanstalk sources. This phase also proves that isolation is workable in the documentation site and generated projects.

### Checklist

- [ ] Migrate every legacy HTML metadata declaration:
  - [ ] `page_title` → `config:title`
  - [ ] `page_description` → `config:description`
  - [ ] `page_lang` → `config:lang`
  - [ ] `page_favicon` → `config:favicon`
  - [ ] `page_body_style` → `config:body_style`
  - [ ] `page_head` → `config:head`
- [ ] Move required imports inside each isolated block
- [ ] Keep outer imports only when normal module code also needs them
- [ ] Resolve import-policy blockers discovered in Phase 0:
  - [ ] move shared metadata constants into an allowed Core or Builder config support package
  - [ ] or inline/compose folded values
  - [ ] do not broaden entry-only import policy
- [ ] Update generated HTML project scaffolding
- [ ] Update HTML builder canonical test projects
- [ ] Update benchmarks and stress fixtures containing legacy constants
- [ ] Update integration manifest entries
- [ ] Replace obsolete success/failure cases rather than retaining parallel legacy paths
- [ ] Keep focused migration-diagnostic fixtures for legacy names
- [ ] Verify docs source pages compile with isolated config blocks
- [ ] Verify duplicate block/key diagnostics in real source fixtures
- [ ] Verify multi-module docs routes retain distinct titles/head content
- [ ] Run docs build/check
- [ ] Run full integration suite
- [ ] Grep repository source for legacy constants and classify every remaining match as:
  - [ ] migration test
  - [ ] migration documentation
  - [ ] stale and must be removed

### Review / audit / style / validation

- [ ] Run docs build/check
- [ ] Run `cargo run -- tests`
- [ ] Run `just validate`
- [ ] Review representative migrated source files for readability
- [ ] Confirm config imports are isolated and not duplicated without reason
- [ ] Confirm no legacy compatibility source remains
- [ ] Confirm benchmark fixtures still represent intended workloads
- [ ] Update active context capsule and `LAST_GOOD_COMMIT`

---

## Phase 9 — update language docs, compiler docs, roadmap and progress matrix

### Context

The source language, build-system boundary, HTML metadata model and Core IO surface have changed. Documentation and deferred-status tracking are part of feature completion.

### Checklist

- [ ] Update `docs/language-overview.md`:
  - [ ] syntax summary for `config:`
  - [ ] active-root-only placement
  - [ ] exactly one block
  - [ ] isolation from surrounding module scope
  - [ ] identical rules to `config.bst`
  - [ ] scoped builder key registration
  - [ ] project and entry config are separate surfaces
  - [ ] HTML V1 keys
  - [ ] initial title versus `io.set_title`
  - [ ] legacy `page_*` removal
- [ ] Update `docs/compiler-design-overview.md`:
  - [ ] shared config compilation owner
  - [ ] file-backed and embedded config sources
  - [ ] header block capture ownership
  - [ ] active/imported root behavior
  - [ ] AST folding through shared config path
  - [ ] resolved entry config on `Module`
  - [ ] explicit absence from HIR
  - [ ] builder interpretation
  - [ ] runtime external call lowering
- [ ] Update docs-site project structure/config page
- [ ] Update HTML page/document metadata docs
- [ ] Update Core IO docs with `io.set_title`
- [ ] Update README example
- [ ] Update migration notes
- [ ] Update `docs/roadmap/roadmap.md`:
  - [ ] link this plan immediately after the module/export plan
  - [ ] keep `#Import` plan after this config foundation
- [ ] Refresh `docs/roadmap/plans/import_values_anonymous_records_plan.md`:
  - [ ] replace stale config registry/parser anchors
  - [ ] require reuse of generalized scoped registry
  - [ ] require reuse of shared config compilation
  - [ ] state whether `#Import` is accepted inside entry config when that later plan lands
- [ ] Update progress matrix with implemented rows:
  - [ ] entry-local isolated `config:` blocks
  - [ ] scoped builder config keys
  - [ ] HTML compile-time page metadata through entry config
  - [ ] removal of legacy `page_*` metadata
  - [ ] `io.set_title` on HTML-JS
- [ ] Update backend matrix cells:
  - [ ] compile-time HTML entry config supported by HTML-JS
  - [ ] compile-time HTML entry config supported by HTML-Wasm document shell
  - [ ] runtime `io.set_title` supported by HTML-JS
  - [ ] runtime `io.set_title` deferred/unsupported on HTML-Wasm
- [ ] Add the deliberately deferred items listed below to roadmap/matrix notes
- [ ] Ensure generated docs/navigation includes any new page or section

### Review / audit / style / validation

- [ ] Run docs build/check
- [ ] Run `just validate`
- [ ] Grep docs for stale claims:
  - [ ] top-level `page_*` constants configure HTML
  - [ ] entry config sees surrounding scope
  - [ ] multiple config blocks merge
  - [ ] HTML-Wasm supports runtime title mutation
- [ ] Confirm roadmap order matches prerequisite relationships
- [ ] Confirm matrix distinguishes implemented, partial and deferred target support
- [ ] Review prose for compiler-facing precision
- [ ] Update active context capsule and `LAST_GOOD_COMMIT`

---

## Phase 10 — final cleanup, boundary audit and performance validation

### Context

The final phase proves there is one config system, no stale builder magic and no unacceptable frontend regression. It removes temporary scaffolding only after all callers are migrated.

### Checklist

- [ ] Delete obsolete code:
  - [ ] project-only parser wrappers superseded by shared config compilation
  - [ ] duplicate value extraction helpers
  - [ ] reserved HIR page metadata scanner
  - [ ] legacy page metadata fallback
  - [ ] unused page-metadata diagnostic variants
  - [ ] temporary adapters introduced during phased wiring
- [ ] Grep code and docs:
  - [ ] `ProjectConfigKeyRegistry` removed or intentionally renamed everywhere
  - [ ] only one config compilation path remains
  - [ ] `page_title`, `page_head` and other `page_*` names remain only in migration coverage
  - [ ] no builder scans `module_constants` for metadata names
  - [ ] no entry config reaches HIR
  - [ ] no second source-tree scan exists
  - [ ] no unsupported runtime title no-op exists
- [ ] Manual stage-boundary audit:
  - [ ] root parser owns block discovery
  - [ ] shared config compiler owns config parsing/folding/shape validation
  - [ ] project consumer owns project setting application
  - [ ] compiled module owns builder-facing entry values
  - [ ] HTML builder owns page meaning
  - [ ] backend owns runtime title lowering
  - [ ] borrow checker and HIR remain unaffected
- [ ] Manual style-guide audit:
  - [ ] module docs explain WHAT/WHY and exclusions
  - [ ] moved code has corrected ownership and names
  - [ ] no compatibility wrappers
  - [ ] no user-input panic paths
  - [ ] no large mixed-responsibility files without justification
  - [ ] no long boolean-heavy APIs
  - [ ] tests are in dedicated test modules/directories
- [ ] Run full validation:
  - [ ] `just validate`
- [ ] Run final performance protocol:
  - [ ] `just bench-frontend-check`
  - [ ] `just bench-check`
  - [ ] `just bench-report`
- [ ] Compare against Phase 0:
  - [ ] no meaningful regression for projects without entry config
  - [ ] config-block projects compile each block once
  - [ ] docs build remains within accepted noise
  - [ ] no new discovery scan
- [ ] Profile any unexplained regression before acceptance
- [ ] Record concise performance conclusions
- [ ] Update progress matrix and roadmap status if implementation is complete
- [ ] Mark all accepted checklist items
- [ ] Refresh active context capsule one final time
- [ ] Record final `LAST_GOOD_COMMIT`

### Review / audit / style / validation

- [ ] Independent review of config parser reuse
- [ ] Independent review of root-role behavior
- [ ] Independent review of HTML metadata security/escaping/raw-head behavior
- [ ] Independent review of runtime host capability behavior
- [ ] Independent documentation and matrix review
- [ ] `just validate` passes or accepted unrelated failures are documented
- [ ] Final plan capsule is reloadable and accurately describes repository state

---

## Deliberately deferred features

These features are in design scope but are **not** implemented by this plan. Add or update them explicitly in `docs/roadmap/roadmap.md` and the nearest progress-matrix rows.

### Entry config language and composition

- multiple `config:` blocks
- block merging or last-write-wins semantics
- per-key append syntax such as `head += ...`
- key-specific merge policies
- config blocks in normal implementation files
- exported or imported entry config
- imported module config automatically affecting the importer
- surrounding module scope capture
- implicit access to outer imports
- project-local or relative imports unless canonical `config.bst` changes through a separate shared-policy decision
- runtime expressions or mutable config values
- config functions or host calls
- automatic project-config defaults overridden by entry config
- automatic precedence for keys valid in both scopes
- config key aliases
- user-defined builder key schemas
- implicit metadata side effects from importing a package

### Structured and typed metadata

- typed HTML head node choices
- structured meta/link/style/script records
- raw-head sanitization policy
- automatic asset tracking from arbitrary head HTML
- anonymous-record-specific config syntax
- `#Import` values inside entry config
- CLI/environment overrides for entry config
- serialization of arbitrary config values
- builder-defined nested config namespaces

The `#Import` and anonymous-record plan owns those features where applicable. This plan only supplies the reusable config compiler and scoped registry foundation.

### Runtime host/window API

- `io.get_title`
- `io.title(...)` alias
- runtime description mutation
- runtime favicon mutation
- runtime language mutation
- runtime head insertion
- runtime body style mutation
- reactive title binding
- automatic title updates from reactive values
- window handles or multiple-window targeting
- HTML-Wasm lowering for `io.set_title`
- native/window backend lowering for `io.set_title`
- a general host capability trait or effect system
- silent fallback/no-op semantics on unsupported hosts

### Performance and tooling

- incremental entry-config compilation cache
- source-hash cache for config blocks
- parallel compilation of independent config blocks
- LSP-specific config block semantic model beyond normal frontend reuse
- editor completion generated from builder config-key schemas
- external schema export for tooling
- config lock/cache metadata

### Explicitly rejected design directions

These should be documented as removed or not planned rather than deferred:

- magic top-level constants selected by spelling
- `html.title #= ...` assignment into an imported namespace
- builder scanning HIR constants to discover metadata
- imports that silently add document metadata
- legacy `page_*` compatibility behavior
- separate parsers for project config and entry config

---

## Completion definition

This plan is complete only when:

- the shared config compiler is the one maintained implementation for both config surfaces
- the scoped registry is the one builder key schema
- one isolated entry block reaches the builder without entering HIR
- HTML initial metadata is fully migrated
- `io.set_title` works on HTML-JS and fails clearly on unsupported targets
- all old page metadata behavior is removed
- repository source and docs are migrated
- roadmap and matrix clearly separate implemented and deferred support
- performance and full validation gates pass
- the active context capsule accurately records the final state
