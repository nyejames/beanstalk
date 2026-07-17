# Project config, imported build values and anonymous records implementation plan

## Purpose

Implement the self-contained project config model, section-aware schemas, the synthetic `@project` interface, project-wide imported build values and anonymous record semantics on top of the canonical module graph.

## Current-state capsule

```text
ACTIVE_PLAN: docs/roadmap/plans/import_values_anonymous_records_plan.md
STATUS: queued
CURRENT_SLICE: Phase 0 - refresh current config, schema, CLI and anonymous-record owners
LAST_GOOD_COMMIT: none until the first implementation slice is accepted
BRANCH: main
IMPLEMENTATION_SCOPE: build system config, frontend declaration syntax, CLI, anonymous records
```

## Hard prerequisites

- final TIR
- canonical module and package graphs
- stable source and semantic identities
- prepared source `#Import` contract shells
- the accepted config and synthetic-interface contracts

This plan must complete before entry-local config blocks.

## Required authority documents

- `docs/compiler-design-overview.md` for frontend stages, synthetic interfaces and AST folding
- `docs/build-system-design.md` for config bootstrap, `@project`, source `#Import` contracts, section schemas and output ownership
- `docs/language-overview.md` for source syntax
- `docs/src/docs/codebase/style-guide/style-guide.bd`, `testing.bd` and `validation.bd`
- `docs/src/docs/progress/#page.bst` for current support
- `docs/roadmap/plans/canonical-module-compilation-and-scoped-packages-plan.md` for the module graph this plan builds on

## Delete rejected design

Remove every statement that says:

- config can import Core or Builder source
- config builds an import graph
- config has support files
- top-level primitive config constants are public globals
- unknown top-level primitive constants are exported
- builder selection is `project #= "html"`
- config uses flat known keys
- records and nested fields are hidden config implementation details
- output folders are global project fields
- `package_folders` exists
- source `#Import` defaults may use general constant expressions
- `#Import` is valid everywhere an ordinary constant is valid without further restriction
- entry and project fields may use a shared scope
- the old config API must remain available

## Accepted config design

See `docs/build-system-design.md` "Self-contained config.bst", "Project record", "Builder and tooling sections" and "Entry-local config: blocks" for full contracts.

- command and builder selected before config schema validation (see "Selected command and capability surface")
- one authored `config.bst` with no source imports or package resolution
- one required open `project` record
- private helper constants declared before use
- top-level builder and tooling records
- all sections folded, but only active sections schema-validated
- active builder project section required, even when empty
- inactive sections discarded after folding
- separate project and entry schemas with no shared fields (`ProjectAndEntry` is explicitly rejected)
- project fields may contain folded scalar values, optionals, nested anonymous const records, collections and folded template strings
- `project.name` required, must be a valid package-style identifier and provides stable project identity
- project fields have no implicit sibling scope (reusable derived values belong in earlier private helper constants)
- builder sections use backend-neutral folded values and cannot declare `#Import`
- builder-owned output settings (HTML defaults: dev `dev`, release `release`)
- final builder selection syntax remains deferred

## Imported build values

See `docs/build-system-design.md` "Direct project #Import fields" and "Source #Import contracts" for full contracts.

- direct project `#Import` fields use the accepted primitive and optional domain: `String`, `Int`, `Float`, `Bool`, `Char` and optional forms
- nested project fields cannot declare imports and do not provide unqualified source input values
- project imports resolve during config folding before Stage 0 applies fields such as `entry_root`
- source contracts are normalised during header syntax preparation into `SourceBuildInputContract` shapes
- source contracts are collected before module AST compilation
- source defaults are limited to a self-contained primitive literal or `none` (no names, templates, operators, calls, casts, projections, collections or records)
- no second general constant evaluator runs in Stage 0
- same-name contracts must agree on primitive type, optionality, required or default state and normalised default value
- fixed project fields are authoritative providers: a same-name source `#Import` using the same primitive type and optionality reads the fixed field value and blocks CLI override
- explicit CLI input cannot override a fixed project field
- unknown inputs are diagnosed only after every selected source contract is known
- resolved source imports become ordinary folded constants in AST, creating no runtime wrapper or HIR category
- CLI and programmatic inputs persist across dev rebuilds

Resolution order (see `docs/build-system-design.md` "Source #Import contracts"):
1. a compatible fixed direct project field (authoritative, cannot be overridden)
2. a resolved direct project `#Import` field
3. explicit CLI or programmatic input for a source-only contract
4. a builder-provided primitive global
5. the shared source default
6. a missing-input diagnostic

Use `current imported-value domain`, not `V1`.

## ProjectGlobalsInterface

See `docs/build-system-design.md` "ProjectGlobalsInterface and @project" for the full contract.

Require:
- permanent `@project` reservation
- stable field identities
- folded backend-neutral values
- source locations
- field-level fingerprints
- project-context provenance
- field-level dependency tracking (a field change invalidates only facts that depend on it)
- explicit imports only (normal project modules and project-owned support packages may import `@project`, never implicitly injected)
- no direct re-export
- no facade exposure of project-private semantic facts
- no AST, HIR or runtime body

Entities that may not claim `@project`: child modules, scoped support packages, dependency aliases, Core packages, Builder packages, binding-backed packages.

## Anonymous records

See `docs/compiler-design-overview.md` "Public-surface validation" for the escape-rejection contract.

Retain and sharpen:
- every runtime literal site has one hidden nominal identity registered in `TypeEnvironment`
- shape equality does not imply type equality (different literal sites never unify by shape)
- runtime anonymous records may use ordinary local struct-style lowering (reuse existing struct field access and lowering)
- runtime anonymous records cannot escape public surfaces: exported signatures, fields, aliases, returns, trait evidence, receiver surfaces
- no receiver methods
- no conformance
- no exported runtime identity
- folded anonymous records become field-access-only const records (reuse existing const-record field projection)
- exported anonymous const records are allowed when fully folded and field-access-only
- no anonymous-specific HIR nodes unless nominal lowering cannot represent the feature
- const-record provenance is preserved

## Non-goals

- no structural typing or shape-based anonymous record unification
- no key aliasing for `#Import`
- no `-D`, `--define`, JSON input or direct OS environment variable syntax
- no lowercase `import` overload for build values
- no runtime `Import` wrapper type
- no compatibility path for the old flat hidden config-key shape
- no Beanstalk-native env-file or general input source until a separate accepted design

## Risks and blockers

- config `#Import` must resolve early enough to affect other config values, while ordinary source `#Import` resolves after config validation
- unknown CLI input validation must wait until both config globals and reachable source contracts are known
- dev server must preserve input values through runtime path resolution, initial build and every rebuild
- `|...|` is already used in parameters, struct declarations, choice payloads, receiver signatures and templates, so expression parser changes must be context-specific
- config validation currently assumes all authored config constants are known flat keys, so grouped config and public user globals must land as one clean break

## Implementation phases

Each phase must leave one coherent path. Reference the named sections in `docs/build-system-design.md` and `docs/compiler-design-overview.md` for full contracts.

### Phase 1: Refresh current config, schema, CLI and anonymous-record owners

Context: this plan depends on canonical module work and final TIR. Refresh all code anchors before implementation.

- Confirm final TIR, canonical module and package graphs, stable identities and prepared source `#Import` contract shells are accepted.
- Record `git rev-parse HEAD`, branch and `git status --short` in the context capsule.
- Refresh the current config parser, schema registry, CLI parser, anonymous-record and const-record owners.
- Run baseline `just validate` and record results.

### Phase 2: Implement hidden nominal anonymous records and const-record folding

Context: grouped config records and docs-site const helper migration need anonymous records. This phase adds parser, type identity and folding foundations.

See `docs/compiler-design-overview.md` "Public-surface validation" for the escape contract.

- Add expression-position `|...|` anonymous record literal parsing that is context-specific (does not conflict with parameters, struct declarations, choice payloads, receiver signatures or templates).
- Register hidden nominal record types in `TypeEnvironment` with source-site identity, ordered fields, canonical `TypeId` and diagnostic display name.
- Each literal site creates a unique hidden type. Different sites never unify by shape.
- Add early escape diagnostics: runtime anonymous record returned, exported through signature, field, alias, receiver method, trait conformance, collection or generic escape.
- Extend const evaluation so anonymous records fold when every field folds. Store as const records with hidden nominal identity.
- Allow exported anonymous const records when fully folded and field-access-only.
- Reuse existing const-record field projection for `record.field`.
- Do not add anonymous-specific HIR nodes unless existing struct lowering cannot represent the feature.

### Phase 3: Replace flat config keys with one section-aware recursive schema owner

Context: config validation currently assumes all authored config constants are known flat keys. Grouped config and public user globals must land as one clean break.

See `docs/build-system-design.md` "Builder and tooling sections" for the section schema contract.

- Extend `ProjectConfigKeyRegistry` or equivalent into a path-aware schema supporting top-level primitive keys, known record names and known record fields.
- Keep separate project and entry schemas with no shared fields.
- Active sections are schema-validated. Unknown fields in active sections are diagnostics.
- Inactive or unavailable sections are parsed, name-resolved and folded but not schema-validated and not retained.
- Duplicate section names are rejected. A section name cannot collide with another top-level constant.
- Builder and tooling sections cannot declare `#Import`.

### Phase 4: Make config single-file and delete its resolver, source set and import graph

Context: accepted design is one authored config source identity with no source imports.

See `docs/build-system-design.md` "Self-contained config.bst" for the full contract.

- Config bootstrap operates on exactly one authored source identity.
- Delete the package resolver, config import graph, config source set and any second project source scan.
- An authored `import` declaration is rejected before path resolution with a structured diagnostic.
- Config uses the ordinary compiler owners for its one file: tokenization, declaration-shell parsing, local declaration ordering, AST semantic checking and folding.
- Config stops after the folded AST boundary. It produces no HIR or borrow facts.
- Delete old flat hidden key compatibility diagnostics. Do not silently accept both shapes.

### Phase 5: Implement direct project imports and input parsing

Context: direct project `#Import` fields and CLI inputs need a typed carrier and resolver.

See `docs/build-system-design.md` "Direct project #Import fields" for the full contract.

- Add `--input name=value` CLI parsing for `build`, `check` and `dev` (repeated, lower_snake_case names, no aliasing, no `-D`, no `--define`, no JSON).
- Define `BuildScalarType` for supported primitive and optional types: `String`, `Int`, `Float`, `Char`, `Bool` and optional forms.
- Use `numeric_text` for CLI `Int` and `Float` parsing. Reject non-finite `Float` values.
- Resolve direct project `#Import` fields in order: explicit CLI input, builder-provided global, folded declaration default, missing-input diagnostic.
- A fixed direct project field is authoritative and blocks CLI override even when source files declare same-name `#Import`.
- Thread inputs through `build_project`, `bootstrap_project_build`, `run_check`, `run_dev_server` and watch rebuild loop state.
- Keep unknown CLI input validation delayed until reachable source contracts are known.

### Phase 6: Implement source contract shells and the project-wide barrier

Context: source `#Import` is intentionally narrow so every project-wide contract can be validated before module AST compilation.

See `docs/build-system-design.md` "Source #Import contracts" for the full contract.

- Header syntax preparation normalises each source contract into a `SourceBuildInputContract` shape.
- Source defaults are limited to a self-contained primitive literal or `none`.
- Same-name contracts must agree on primitive type, optionality, required or default state and normalised default value.
- The barrier validates all contracts in the command's selected source graph before module AST compilation.
- No second general constant evaluator runs in Stage 0.
- The resolved value enters module AST as an ordinary folded constant. It creates no runtime wrapper or HIR category.

### Phase 7: Implement ProjectGlobalsInterface and @project visibility

Context: the folded `project` record produces a specialised immutable interface under the permanently reserved `@project` root.

See `docs/build-system-design.md` "ProjectGlobalsInterface and @project" for the full contract.

- Build the immutable `ProjectGlobalsInterface` with stable field identities, folded values, source locations, field fingerprints, provenance and field-level dependency tracking.
- `@project` is permanently reserved. It is never implicitly injected.
- Normal project modules and project-owned support packages may explicitly import `@project`.
- `@project` cannot be directly re-exported.
- The external project package facade rejects prohibited project-context exposure.
- The interface has no AST, HIR or runtime body.

### Phase 8: Move output settings into active builder sections

Context: output folders are builder-owned, not global project fields.

See `docs/build-system-design.md` "Output ownership" for the full contract.

- Artefact builders own output-path settings and defaults in their private project config section.
- Remove global `dev_folder` and `output_folder` project fields.
- HTML defaults remain: development `dev`, release `release`.
- A selected builder may override defaults through its active project section.
- Every output root must be relative to the project root, outside `entry_root`, free of parent traversal and contained by the project output policy.

### Phase 9: Thread resolved config and inputs through build, check and dev

Context: resolved config and inputs must flow through all command paths consistently.

- Thread resolved config values, `ProjectGlobalsInterface` and build inputs through `build`, `check` and `dev`.
- Ensure dev server preserves input values through runtime path resolution, initial build and every rebuild.
- Ensure `bean check --input` and `bean build --input` resolve frontend constants identically.
- Ensure HTML-Wasm either accepts compile-time-only cases or rejects unsupported runtime anonymous record use before backend lowering.

### Phase 10: Delete flat globals, builder selector, global output folders and config compatibility paths

Context: the refactor is not complete while old flat config, builder selector or global output folder paths remain.

- Delete `project #= "html"` builder selector and its diagnostics.
- Delete global `dev_folder` and `output_folder` fields.
- Delete `package_folders` config field.
- Delete flat config key registry and flat key validation.
- Delete old flat hidden key compatibility diagnostics.
- Delete named config support structs.
- Delete any remaining config import graph, source set and resolver paths.
- Do not leave compatibility wrappers.

### Phase 11: Migrate scaffolds, fixtures, docs and the progress matrix

Context: documentation and scaffolding must teach the accepted config model.

- Update `bean new` scaffold `config.bst` output to the grouped record shape with `project` record, builder sections and direct `#Import` fields.
- Update all integration fixtures containing old flat hidden keys.
- Update `docs/language-overview.md` with config, `#Import` and anonymous record source semantics.
- Update project-structure and imported-build-values source pages.
- Update progress matrix rows for anonymous records, imported build values, grouped config, `@project` and deferred input syntaxes.
- Rebuild generated documentation through the compiler.

## Old owners and paths to remove

- flat config key registry and flat key validation
- config import graph, source set and resolver
- `project #= "html"` builder selector
- global `dev_folder` and `output_folder` fields
- `package_folders` config field
- old flat hidden key compatibility diagnostics
- named config support structs

## Required tests

Cover:

- config import rejection before path resolution
- project record validation
- helper ordering
- active and inactive section folding
- active schema errors
- unknown inactive sections
- direct project imports
- fixed project providers
- conflicting source contracts
- restricted source defaults
- unknown CLI inputs
- dev rebuild input retention
- `@project` collisions and field-level dependencies
- facade provenance rejection
- runtime anonymous record non-escape
- exported anonymous const records
- no anonymous-specific HIR path

## Documentation and progress-matrix impact

- update `docs/language-overview.md` with config, `#Import` and anonymous record source semantics
- update `docs/build-system-design.md` only if a durable config contract is confirmed missing
- update project-structure and imported-build-values source pages
- update scaffold `config.bst` output
- progress matrix rows: anonymous records, imported build values, grouped project config, `@project`, deferred input syntaxes

## Validation requirements

Each code-bearing phase runs:

```bash
cargo fmt
just validate
```

Run the documentation release build when source docs change.

## Final architecture audit

Before marking this plan complete, verify:

- config is one self-contained file with no source imports
- section-aware schemas validate only active sections
- `@project` is permanently reserved with no direct re-export
- source `#Import` contracts are normalised and validated before module AST
- fixed project fields block CLI override
- anonymous records use hidden nominal identity with no structural typing
- no flat config, builder selector or global output folder path remains
- no runtime `Import` wrapper or anonymous-specific HIR path remains
