# `#Import` values and anonymous records implementation plan

## Active context capsule

ACTIVE_PLAN:
- `docs/roadmap/plans/import_values_anonymous_records_plan.md`

CURRENT_SLICE:
- Phase: Phase 0 — refresh after prerequisite plans
- Checklist item: confirm prerequisite plan state, refresh code anchors, and record local baseline before implementation
- Goal: implement after TIR finalisation and after the hash-root/export-block module-system plan, before the HTML project builder Wasm backend plan.
- Non-goals:
  - No structural typing.
  - No shape-based anonymous record unification.
  - No key aliasing for `#Import`.
  - No `-D`, `--define`, JSON input, or direct OS environment variable syntax.
  - No lowercase `import` overload for build values.
  - No runtime `Import` wrapper type.
  - No compatibility path for the old flat hidden config-key shape.
  - No compatibility path for alternate config filenames. The preceding hash-root/module plan makes `config.bst` the only project config filename.

LAST_GOOD_COMMIT:
- `none` until the first implementation slice is accepted locally.

CURRENT_WORKTREE_STATE:
- Clean / known changes:
  - Unknown. Run `git status --short` before the first implementation slice.
- Branch:
  - Target branch: `templates-refactor`. Verify locally with `git branch --show-current`.
- Dedicated worker worktrees:
  - None known. Add paths here if coding agents use separate worktrees.

RELEVANT_DOCS_THIS_SLICE:
- `AGENTS.md` — check locally before each implementation slice if present.
- `docs/codebase-style-guide.md` — required for every implementation slice.
- `docs/compiler-design-overview.md` — required for Stage 0, frontend, config, and build-system boundary changes.
- `docs/language-overview.md` — required for syntax, semantics, diagnostics, and tests.
- `docs/memory-management-design.md` — required only when anonymous record runtime lowering or field mutation touches borrow/ownership behavior.
- `docs/roadmap/plans/final-tir-completion-plan.md` — prerequisite; confirm complete before implementation.
- `docs/roadmap/plans/hash-root-export-block-module-system-plan.md` — prerequisite; confirm complete before implementation.
- `docs/roadmap/roadmap.md` — keep plan order accurate.
- `docs/src/docs/progress/#page.bst` — update matrix rows and deferred/not-planned notes.
- `docs/src/docs/project-structure/#page.bst` — update config structure and link the new `#Import` page.
- `docs/src/docs/imported-build-values/#page.bst` — new user-facing page.
- `docs/src/docs/templates/#page.bst` — read only if anonymous record examples interact with templates/TIR.

RELEVANT_CODE:
- `src/build_system/create_project_modules/source_tree_index.rs` or the post-hash-root equivalent: Stage 0 source tree/config/module-root discovery; consume it, do not add another scan.
- `src/build_system/project_config.rs`: Stage 0 config parse/load entry point; should load canonical `config.bst` after the prerequisite module plan.
- `src/build_system/project_config/**`: config parsing, validation, grouped record extraction, public build globals, and migration diagnostics.
- `src/builder_surface/definition.rs`: builder-declared frontend surface; prefer adding builder global metadata here rather than a new `BackendBuilder` method unless this becomes clearly awkward.
- `src/builder_surface/config_key_registry.rs`: current config-key schema; extend into top-level keys plus record-field paths instead of adding a parallel whitelist.
- `src/projects/settings.rs::Config`: final project settings consumed by builders; add public build globals and canonical setting locations.
- `src/projects/html_project/html_project_builder.rs`: HTML builder config-key registration and validation; after grouping, keep builder parsers consuming final `Config` rather than AST config records.
- `src/projects/html_project/new_html_project/start_page_scaffolding.rs::config_template`: update generated `config.bst`.
- `src/projects/cli.rs`: manual command parser; add repeated `--input name=value` for `build`, `check`, and `dev` without introducing a CLI framework.
- `src/compiler_frontend/mod.rs::Flag`: do not store input values in `Flag`; introduce named build/check/dev options.
- `src/build_system/build.rs::build_project` and `bootstrap_project_build`: thread build inputs and resolved build globals through Stage 0/frontend compilation.
- `src/projects/check.rs::CheckOptions`: add build inputs for `bean check --input`.
- `src/projects/dev_server/server.rs` and `src/projects/dev_server/build_loop.rs`: persist inputs through initial build, runtime path resolution, and every rebuild.
- `src/compiler_frontend/declaration_syntax/**`: parse `#Import of T` as constant-source syntax, not a normal type.
- `src/compiler_frontend/headers/**`: record `#Import` shells and type dependency edges after the hash-root role/export-block refactor.
- `src/compiler_frontend/ast/**`: resolve `#Import`, register hidden anonymous record types, const-fold records, and reject invalid escapes.
- `src/compiler_frontend/ast/templates/tir/**`: after TIR finalisation, consume final TIR folding/handoff APIs; do not revive old template authority.
- `src/compiler_frontend/ast/field_access/**`: reuse existing field access for anonymous records.
- `src/compiler_frontend/hir/**`: do not add anonymous-specific HIR nodes unless existing nominal struct paths cannot support the feature.
- `src/compiler_frontend/numeric_text/**`: reuse source numeric grammar for CLI `Int`/`Float` parsing.
- `src/compiler_frontend/compiler_messages/**`: add structured diagnostics and stable diagnostic codes.
- `tests/cases/manifest.toml`: add integration fixtures for anonymous records, grouped config, `#Import`, CLI inputs, and diagnostics.

ACCEPTANCE_CRITERIA:
- Anonymous record literals compile in supported local/runtime and const contexts.
- Each anonymous record literal site creates a hidden nominal type; shape-based unification is rejected.
- Runtime anonymous record types cannot be returned, exposed through exported signatures/fields/aliases, used for receiver methods, or used for trait conformance.
- Exported anonymous const records are allowed when fully folded and exposed only as field-access-only compile-time const records.
- `#Import of T` works anywhere ordinary `#` constants are valid; it does not broaden where ordinary constants are legal.
- Supported V1 imported types are exactly `String`, `Int`, `Float`, `Char`, `Bool`, and optional forms of those.
- `bean build`, `bean dev`, and `bean check` accept repeated `--input name=value`.
- CLI inputs override only explicit `#Import` declarations; fixed config globals block override with a specific diagnostic.
- Top-level primitive config constants in `config.bst` become public build globals visible to `#Import`; records and nested fields do not.
- The default config scaffold uses the grouped record shape.
- The implementation consumes the post-hash-root source tree/config discovery result and does not add another expensive project scan.
- Docs, docs-site examples, progress matrix, roadmap, and integration tests are updated.
- Source/config/build-input semantic failures use `CompilerDiagnostic`; internal/tooling failures use `CompilerError`.
- CLI command-shape errors may stay on the existing CLI string-error path unless a CLI diagnostic owner already exists.
- `just validate` passes, or accepted unrelated failures are recorded in this capsule.

DECISIONS_ALREADY_MADE:
- decision: Build-value syntax is `name #Import of T = default` or `name #Import of T`.
  - reason: Uses existing `#` compile-time declaration shape and existing `of` type-constructor syntax without reserving a broad keyword.
  - source/user/date: User design interview, 2026-06-30.
- decision: `Import of T` is only valid immediately after `#` in constant declaration type position.
  - reason: It is a compile-time value source annotation, not a runtime type.
  - source/user/date: User design interview, 2026-06-30.
- decision: `#Import of T` is valid anywhere ordinary `#` constants are valid.
  - reason: It should be a general compile-time value insertion mechanism, not config-only or top-level-only.
  - source/user/date: User design interview, 2026-06-30.
- decision: V1 imported values are primitive scalars plus optionals only.
  - reason: Avoids general Beanstalk value serialization in the build input layer.
  - source/user/date: User design interview, 2026-06-30.
- decision: CLI syntax is only repeated `--input name=value`.
  - reason: Explicit and strict; no shorthand or aliasing.
  - source/user/date: User design interview, 2026-06-30.
- decision: Future env-file/general input support may be pursued through a Beanstalk-native design, likely extending config.
  - reason: Avoid external config languages and shell-specific source semantics.
  - source/user/date: User design interview, 2026-06-30.
- decision: Key aliasing, JSON inputs, `-D`, and `--define` will not be supported.
  - reason: Keep source/input mapping strict and non-ambiguous.
  - source/user/date: User design interview, 2026-06-30.
- decision: Repeated `#Import` declarations with the same name must define one identical project-wide contract when they are all `#Import` declarations.
  - reason: Prevents conflicting defaults and type expectations.
  - source/user/date: User design interview, 2026-06-30.
- decision: Top-level primitive config constants are public build globals; records and nested fields are not.
  - reason: Config structure visibly encodes source visibility.
  - source/user/date: User design interview, 2026-06-30.
- decision: Unknown top-level primitive config constants are allowed as user-defined public project globals.
  - reason: Lets config define project-wide constants without making them builder settings.
  - source/user/date: User design interview, 2026-06-30.
- decision: CLI override requires explicit `#Import`.
  - reason: Fixed config constants must not be silently overridden by earlier build stages.
  - source/user/date: User design interview, 2026-06-30.
- decision: A fixed top-level config constant blocks CLI override even when ordinary source files declare same-name `#Import`.
  - reason: `config.bst` remains authoritative for fixed project globals.
  - source/user/date: User design interview, 2026-06-30.
- decision: Anonymous records are part of this plan.
  - reason: Grouped config records depend on the general language feature.
  - source/user/date: User design interview, 2026-06-30.
- decision: Anonymous fields may be inferred or explicitly annotated, for example `red String = ...`.
  - reason: Docs can replace named support structs that existed only to instantiate const records.
  - source/user/date: User addition, 2026-06-30.
- decision: Exported anonymous const records are allowed when fully folded and field-access-only.
  - reason: Needed for docs/style modules that export const-record helper groups without public runtime anonymous record types.
  - source/user/date: Follow-up plan review, 2026-07-06.
- decision: Default config shape uses public metadata plus grouped hidden builder settings.
  - reason: Separates source-visible build globals from internal/build-layout settings by source shape.
  - source/user/date: User design interview, 2026-06-30.

BLOCKERS / RISKS:
- TIR finalisation may move template folding/handoff paths. Refresh code anchors after completion before touching anonymous record field initializers that contain templates.
- The hash-root/export-block plan changes config filename, module root roles, source discovery, and public API exposure. Do not implement this plan against pre-refactor config or `#mod.bst` assumptions.
- `|...|` is already used in parameters, struct declarations, choice payloads, receiver signatures, and templates; expression parser changes must be context-specific.
- Config validation currently or recently assumed all authored config constants are known flat keys; grouped config and public user globals must land as one clean break.
- Config `#Import` must resolve early enough to affect other config values; ordinary source `#Import` can resolve after config validation.
- Unknown CLI input validation must wait until both config globals and reachable source contracts are known.
- Dev server must preserve input values through runtime path resolution, initial build, and every rebuild.
- If CLI/build-input diagnostics need source locations, add an explicit command-line/input location model rather than inventing fake file paths.

VALIDATION_STATE:
- last command: none in this artifact.
- result: not run.
- known unrelated failures: unknown.

DOCS_IMPACT:
- progress matrix needed: yes.
- other docs stale:
  - `docs/language-overview.md`
  - `docs/compiler-design-overview.md`
  - `docs/src/docs/project-structure/#page.bst`
  - new `docs/src/docs/imported-build-values/#page.bst`
  - scaffold/default config snippets
  - docs-site examples using named support structs solely to export const-record instances.
- authorized docs updates:
  - yes; update compiler-facing docs, roadmap/matrix, docs site, examples, and tests.

NEXT_ACTION:
- Confirm TIR finalisation plan is complete.
- Confirm hash-root/export-block module-system plan is complete.
- Refresh this capsule from the local `templates-refactor` worktree before the first code slice.

---

## Current repo and prerequisite context

This plan is intentionally after two prerequisite plans on `templates-refactor`:

1. **Final TIR completion**
   - The template system should be AST-local and TIR-authoritative.
   - Template syntax lowers through final TIR composition/folding/handoff into folded strings or runtime expression payloads.
   - HIR must not carry TIR registries, stores, views, overlays, slot-routing internals, or directive data.

2. **Hash-root module files and `export:` blocks**
   - `config.bst` is the canonical project config file.
   - `config.bst` is the only project config filename. No alternate filename receives config-specific handling.
   - Any non-config `#*.bst` file is a cosmetic hash-root module file.
   - A directory has at most one non-config hash-root file.
   - Public APIs are declared inside a module-root `export:` block.
   - Source tree discovery should use a single Stage 0 source tree index for config and module roots.

Implementation must preserve those outcomes. Do not reintroduce alternate config filenames, `#mod.bst`-specific public API logic, direct hash-file imports, inline export compatibility, or duplicate Stage 0 source tree scans.

Expected architecture to preserve after prerequisites:

- Stage 0 owns source tree discovery, config loading, build globals, source-package discovery and frontend input preparation.
- The path resolver consumes source-tree/module-root data; it should not perform hidden filesystem scans during construction.
- Header parsing owns top-level declaration discovery, module-root role handling, `export:` block public/private metadata, import shells, and dependency edges.
- AST owns semantic type resolution, constant folding, template folding/handoff consumption, hidden anonymous type registration, and `#Import` constant resolution.
- HIR receives only valid typed runtime constructs and folded compile-time metadata.
- Backends consume final `Config` and compiled modules; they do not rediscover config, imports, module roots, or template syntax.

Implementation principle: prefer extending existing owner modules with clearer types over adding broad new abstraction layers.

---

## Final design

### Anonymous records

Expression syntax:

```beanstalk
settings = |
    title = "Docs",
    release = true,
|

typed_settings = |
    title String = "Docs",
    release Bool = true,
|
```

Rules:

- Expression-position `| ... |` creates an anonymous record literal.
- Every field requires an initializer.
- Field type annotations are optional.
- Annotated fields use the annotation as the receiving type for the initializer.
- Unannotated fields infer from the initializer.
- Empty anonymous records are rejected in V1.
- Duplicate field names are rejected.
- Field order is preserved.
- Each literal source site creates a unique hidden nominal type.
- Matching field shape does not imply type compatibility.
- Anonymous records support field access.
- Mutable anonymous record bindings support field writes only if ordinary struct field mutation already supports the same operation.
- Const anonymous records fold into const records.
- Exported anonymous const records are allowed when fully folded and exposed only as field-access-only compile-time const records.
- Runtime anonymous record types cannot appear in exported signatures, fields, aliases, returns, trait evidence, or receiver surfaces.
- Returning anonymous records is rejected.
- Receiver methods and trait conformances for anonymous records are rejected.
- Anonymous records in collections/hashmaps and generic propagation are deferred unless the implementation can prove they stay local and non-escaping. Prefer rejection in V1.
- Equality support is not added.

Implementation target:

- Register hidden nominal types in `TypeEnvironment`.
- Reuse existing struct field/member access and lowering where practical.
- Reuse existing const-record field projection for anonymous const records.
- Do not add anonymous-specific HIR nodes unless no existing struct path can support the feature.
- Render diagnostics with source-site names such as `<anonymous record at src/#home.bst:12:5>` or the post-prerequisite equivalent path.

Docs-site migration example:

```beanstalk
export:
    palette #= |
        red String = [$html:<span style="color: light-dark(hsl(0, 84%, 35%), hsl(0, 73%, 65%));">[$slot]</span>],
        highlight String = [$html:<span style="color: light-dark(hsl(44, 82%, 29%), hsl(65, 80%, 62%));">[$slot]</span>],
    |
;
```

This replaces named support structs that existed only to produce one exported const record instance. Do not convert examples where the named struct is teaching nominal struct syntax.

### Imported build values

Syntax:

```beanstalk
version #Import of String = "0.1.0"
release_build #Import of Bool = false
commit_sha #Import of String
maybe_channel #Import of String? = none
```

Rules:

- `Import of T` is syntax attached to `#` constant declarations.
- It is not a normal type and never becomes a runtime wrapper.
- It is valid only where ordinary `#` constants are already valid. Do not add new ordinary constant contexts as part of this plan.
- The constant’s semantic type is `T`.
- Required imports omit the initializer.
- Ordinary non-import constants must still be initialized.
- Defaults must fully fold and match `T`.
- Same-name `#Import` declarations across reachable project source must have an identical contract:
  - same name;
  - same primitive/optional type;
  - same required/default status;
  - same default value, if present.
- Same-name source `#Import` declarations may read a fixed config public global with the same name when the type matches, but CLI override remains blocked by that fixed config global.

Supported V1 types:

```text
String
Int
Float
Char
Bool
String?
Int?
Float?
Char?
Bool?
```

Unsupported/deferred:

- collections;
- maps;
- anonymous records;
- nominal structs;
- choices;
- templates as distinct value types;
- external opaque types;
- generic parameters;
- user-defined aliases unless they are resolved before validation to one of the supported primitive/optional primitive types;
- future numeric widths until that numeric plan lands.

Provider precedence for each `#Import` declaration:

1. explicit CLI/build input;
2. builder-provided global value;
3. validated public config global;
4. declaration default;
5. diagnostic.

Config-specific nuance:

- `config.bst` `#Import` declarations resolve before config application using CLI/build inputs, builder globals, and local defaults.
- `config.bst` cannot read a validated public config global from the same file as a provider because public config globals are produced by config validation.
- Ordinary source `#Import` declarations can read validated public config globals after config validation.
- A fixed public config global blocks CLI override even if ordinary source files contain same-name `#Import` declarations.

### CLI inputs

V1 syntax:

```bash
bean build docs --input version=0.1.1 --input release_build=true --release
bean dev docs --input version=dev
bean check docs --input version=0.1.1
```

Rules:

- `--input name=value` is the only accepted form.
- `--input` may appear multiple times.
- Names must be valid lower_snake_case Beanstalk value identifiers.
- Duplicate CLI input names are errors before compilation.
- Missing value after `--input` is an error.
- Missing `=` is an error.
- Empty name is an error.
- `String` receives raw shell text after `=`.
- `Int` and `Float` use Beanstalk numeric text grammar, not Rust/JS parsing shortcuts.
- `Float` rejects non-finite values.
- `Bool` accepts only `true` or `false`.
- `Char` accepts exactly one Unicode scalar value.
- `none` is accepted only for optional imports.
- Unknown CLI inputs are errors after reachable config/source contracts are known.

Never supported:

- key aliasing;
- `-D`;
- `--define`;
- JSON input;
- direct OS environment lookup syntax in source.

Future input source work should be Beanstalk-native, likely through `config.bst` or a companion file such as `env.bst`/`#env.bst` only after a separate accepted design.

### Config visibility and grouped settings

Target `config.bst` shape:

```beanstalk
project #= "html"

name #= "html_project"
version #Import of String = "0.1.0"
license #= "MIT"
page_url_style #= "trailing_slash"

paths #= |
    entry_root = "src",
    dev_folder = "dev",
    output_folder = "release",
    package_folders = {"lib"},
|

limits #= |
    template_const_loop_iteration_limit = 10_000,
|

html #= |
    origin = "/beanstalk",
    html_lang = "en",
    html_title_postfix = " | Beanstalk",
    redirect_index_html = true,
|
```

Rules:

- Top-level primitive `#` constants in `config.bst` are public build globals.
- Top-level primitive `#Import` constants are public build globals and are CLI-overridable.
- Unknown top-level primitive constants are allowed as user-defined public globals.
- Top-level records are config records, not public build globals.
- Nested record fields are never visible to `#Import`.
- Builder/internal settings must live in known config records or known top-level primitive keys.
- Unknown top-level records are diagnostics.
- Unknown nested fields inside known records are diagnostics.
- Old flat hidden keys are rejected with migration diagnostics. Do not silently accept both shapes.
- Project config filename handling is not part of this plan. The preceding module/export plan owns the hard switch to `config.bst`.

CLI override strictness:

```beanstalk
version #Import of String = "0.1.0"
```

can be overridden by:

```bash
bean build docs --input version=0.1.1
```

but:

```beanstalk
version #String = "0.1.0"
```

blocks:

```bash
bean build docs --input version=0.1.1
```

and emits a diagnostic suggesting:

```beanstalk
version #Import of String = "0.1.0"
```

This block also applies when ordinary source files contain same-name `#Import` declarations.

### Module/export interaction

After the hash-root/export-block plan, public declarations live in an `export:` block in a module root file. This plan must follow that model.

Valid exported compile-time const record:

```beanstalk
export:
    palette #= |
        red String = [$html:<span style="color: red;">[$slot]</span>],
        highlight String = [$html:<span style="color: gold;">[$slot]</span>],
    |
;
```

Rejected exported runtime anonymous record type exposure:

```beanstalk
export:
    make_palette || -> <anonymous record>: -- user cannot write this type; semantic exposure rejected
        return |
            red = "red",
        |
    ;
;
```

The exact syntax of the rejected example may differ because the user cannot spell the hidden type. The diagnostic should target the return expression/signature boundary where the anonymous runtime type would escape.

---

## Architecture simplification targets

Use these constraints to keep the implementation direct.

- Do not represent `Import` as a `DataType`/`TypeId`. Parse it as constant-source syntax and resolve the inner type normally.
- Do not put raw or typed inputs inside `Flag`. Introduce explicit `BuildOptions` / `CheckOptions` / `DevServerOptions` fields.
- Prefer adding a builder-global registry to `BuilderSurface` over adding a new `BackendBuilder` method. `BuilderSurface` is already the builder-declared frontend surface.
- Keep build input parsing and resolution in one small owner, for example `src/build_system/build_inputs.rs` plus submodules only if it grows.
- Reuse `numeric_text` for CLI `Int`/`Float` parsing.
- Reuse `Config.setting_locations` with dotted keys for nested config locations before adding nested location structures.
- Extend `ProjectConfigKeyRegistry` into a path-aware schema instead of adding a separate public/global whitelist.
- Reuse existing const-record field projection for anonymous const records.
- Reuse existing nominal struct field access/lowering for anonymous runtime records. Hidden type identity belongs in `TypeEnvironment`; HIR should not need a separate anonymous-record concept.
- Keep builders consuming final `Config`, not AST config records.
- Consume the post-hash-root source tree index for config/module discovery; do not add another project scan for `#Import` or config globals.
- Delete old config compatibility paths once the new grouped shape is wired.
- Do not revive old template authority after TIR finalisation. Anonymous record field initializers that contain templates must use final TIR folding/handoff paths.

---

# Phased implementation checklist

## Phase 0 — refresh after prerequisite plans

### Context

This phase refreshes the reloadable context after TIR finalisation and hash-root/export-block work. It should not change compiler behavior.

### Checklist

- [ ] Confirm the TIR finalisation plan is complete.
- [ ] Confirm the hash-root/export-block module-system plan is complete.
- [ ] Confirm canonical config filename is `config.bst` locally.
- [ ] Confirm public API syntax is `export:` locally.
- [ ] Confirm Stage 0 source tree/config/module-root discovery owner after the hash-root plan.
- [ ] Refresh the active context capsule:
  - [ ] `git branch --show-current`
  - [ ] `git rev-parse HEAD`
  - [ ] `git status --short`
  - [ ] worker worktree paths, if any.
- [ ] Check whether `AGENTS.md` exists locally.
- [ ] Read:
  - [ ] `docs/codebase-style-guide.md`
  - [ ] `docs/compiler-design-overview.md`
  - [ ] `docs/language-overview.md`
  - [ ] `docs/roadmap/plans/final-tir-completion-plan.md`
  - [ ] `docs/roadmap/plans/hash-root-export-block-module-system-plan.md`
  - [ ] this plan.
- [ ] Refresh `RELEVANT_CODE` after prerequisite code movement.
- [ ] Run baseline validation:
  - [ ] `just validate`
- [ ] Record validation state in the capsule.

### Review / audit / validation

- [ ] Confirm only plan/capsule fields changed, if any.
- [ ] Confirm no behavior/source changes.
- [ ] Confirm prerequisite architecture facts are true in the local worktree.

---

## Phase 1 — anonymous record syntax and hidden nominal identity

### Context

Grouped config and docs-site const helper migration need anonymous records. This phase adds parser/type identity foundations and escape diagnostics. Keep behavior narrow and avoid backend churn unless existing struct paths make it trivial.

### Checklist

- [ ] Find the expression parser owner for primary expressions and `|...|` contexts after TIR finalisation.
- [ ] Add an AST representation for anonymous record literals:
  - [ ] whole literal location;
  - [ ] ordered fields;
  - [ ] field name and location;
  - [ ] optional parsed type annotation;
  - [ ] initializer expression;
  - [ ] initializer location.
- [ ] Make parsing context-specific so expression-position `|...|` does not conflict with:
  - [ ] function parameters;
  - [ ] struct declarations;
  - [ ] choice payloads;
  - [ ] receiver signatures;
  - [ ] template syntax.
- [ ] Support field forms:
  - [ ] `name = value`
  - [ ] `name Type = value`
- [ ] Reject:
  - [ ] empty anonymous records;
  - [ ] missing initializer;
  - [ ] duplicate field names;
  - [ ] invalid field names;
  - [ ] unsupported trailing comma shape if inconsistent with existing syntax rules.
- [ ] Register hidden nominal record types in `TypeEnvironment`:
  - [ ] source-site identity;
  - [ ] ordered fields;
  - [ ] canonical `TypeId`;
  - [ ] diagnostic display name.
- [ ] Resolve field types:
  - [ ] annotated field resolves to a canonical `TypeId` and receives/coerces initializer;
  - [ ] unannotated field uses initializer natural type.
- [ ] Preserve nominal identity:
  - [ ] same literal site has one type;
  - [ ] different sites never unify by shape.
- [ ] Add early escape diagnostics for V1:
  - [ ] runtime anonymous record returned from a function;
  - [ ] runtime anonymous record exposed through exported signatures/fields/aliases;
  - [ ] receiver method target;
  - [ ] trait conformance target;
  - [ ] type alias target;
  - [ ] generic escape if not locally provable;
  - [ ] collection/map element if deferred.
- [ ] Do not reject fully folded exported anonymous const records.

### Tests

- [ ] Unit/parser tests for literal parsing and context separation.
- [ ] Unit/type tests for source-site hidden identity if integration output cannot inspect it.
- [ ] Integration cases:
  - [ ] `anonymous_record_local_field_access`
  - [ ] `anonymous_record_typed_fields`
  - [ ] `anonymous_record_duplicate_field_rejected`
  - [ ] `anonymous_record_empty_rejected`
  - [ ] `anonymous_record_shape_not_structural`
  - [ ] `anonymous_record_return_rejected`
  - [ ] `anonymous_record_exported_signature_rejected`
  - [ ] `anonymous_record_exported_field_type_rejected`

### Review / audit / validation

- [ ] Check parser code for explicit context handling, not broad token heuristics.
- [ ] Check semantic decisions use `TypeId`, not `DataType`.
- [ ] Check diagnostics are structured `CompilerDiagnostic` values with stable codes.
- [ ] Check no user-input `panic!`, `todo!`, or unsafe `.unwrap()`.
- [ ] Run targeted tests, then `cargo clippy`.
- [ ] Update capsule validation state.

---

## Phase 2 — anonymous const records, TIR interaction, field access, and runtime lowering

### Context

This phase makes anonymous records useful in config, docs, and ordinary local code. It must use final TIR APIs for template-containing field initializers and must not revive old template authority.

### Checklist

- [ ] Extend const evaluation so anonymous records fold when every field folds.
- [ ] Store folded anonymous records as const records with hidden nominal identity.
- [ ] Reuse existing const-record field projection for `record.field`.
- [ ] Allow exported anonymous const records when fully folded and field-access-only.
- [ ] Ensure anonymous const record fields that contain templates use final TIR folding APIs.
- [ ] Ensure runtime anonymous record fields that contain runtime templates receive normal runtime expression payloads after TIR handoff.
- [ ] Do not carry TIR registry/store/view/overlay/directive data into HIR or `Module`.
- [ ] Support runtime/local field access through existing field-access logic.
- [ ] Support mutable field writes only through ordinary mutable-place rules.
- [ ] Lower runtime anonymous records through existing nominal struct/object lowering.
- [ ] Avoid anonymous-specific HIR nodes unless the existing struct path cannot represent the value.
- [ ] Reject unsupported contexts before HIR:
  - [ ] returns;
  - [ ] exported runtime type surfaces;
  - [ ] collections/maps if deferred;
  - [ ] generic escape;
  - [ ] external/backend boundaries.
- [ ] Ensure HTML-Wasm either works through existing struct lowering or receives a normal target-feature diagnostic before backend lowering.
- [ ] Convert a small docs-site style/palette example to anonymous records only after tests pass.

### Tests

- [ ] `anonymous_const_record_projection`
- [ ] `anonymous_const_record_template_fields`
- [ ] `anonymous_const_record_export_success`
- [ ] `anonymous_record_field_annotation_mismatch`
- [ ] `anonymous_record_mutable_field_write` if mutation is supported.
- [ ] `anonymous_record_collection_rejected` if collections are deferred.
- [ ] A docs-style `palette #= | ... |` integration case.

### Review / audit / validation

- [ ] Audit const folding for runtime values leaking into const records.
- [ ] Audit TIR usage: no old template authority, no current-state materialization fallback reintroduced by this feature.
- [ ] Audit HIR lowering for valid `TypeId`s and no sentinel/anonymous-only hacks.
- [ ] Audit borrow checker impact if field writes are enabled.
- [ ] Run targeted anonymous-record tests and `cargo run -- tests --backend html`.
- [ ] Update capsule validation state.

---

## Phase 3 — build input data model, CLI parsing, and build API plumbing

### Context

This phase creates the carrier for raw CLI inputs and builder globals. It should not depend on every `#Import` semantic detail being complete. It must follow the post-hash-root Stage 0/source-tree-index shape and avoid adding another scan.

### Checklist

- [ ] Add a focused build input owner, preferably:
  - [ ] `src/build_system/build_inputs.rs`, or
  - [ ] `src/build_system/build_inputs/mod.rs` only if multiple files are justified.
- [ ] Define raw CLI input types:
  - [ ] name;
  - [ ] raw value string;
  - [ ] CLI argument index or other input location metadata.
- [ ] Define typed value/type enums:
  - [ ] `BuildScalarType` for supported primitive/optional types;
  - [ ] `BuildScalarValue` for `String`, `Int`, `Float`, `Char`, `Bool`, and `None`.
- [ ] Keep raw strings outside `StringTable`; intern only when entering frontend/config diagnostic boundaries.
- [ ] Add builder-provided global registry to `BuilderSurface` if practical.
- [ ] Add repeated `--input name=value` parsing in `src/projects/cli.rs` for:
  - [ ] `build`;
  - [ ] `check`;
  - [ ] `dev`.
- [ ] Reject CLI parse mistakes early on the existing CLI command-error path unless a typed CLI diagnostic owner already exists:
  - [ ] missing value after `--input`;
  - [ ] missing `=`;
  - [ ] empty name;
  - [ ] invalid lower_snake_case name;
  - [ ] duplicate input name.
- [ ] Introduce named build options instead of adding positional parameters:
  - [ ] `BuildOptions { flags, inputs }` or equivalent.
- [ ] Thread inputs through:
  - [ ] `build_project`;
  - [ ] `bootstrap_project_build`;
  - [ ] `BuildBootstrap` if needed;
  - [ ] post-hash-root source tree/config discovery path;
  - [ ] `run_check` / `CheckOptions`;
  - [ ] `run_dev_server` / `DevServerOptions` or a nested dev build options field;
  - [ ] `resolve_dev_runtime_paths`;
  - [ ] `ProjectBuildExecutor` and `DevBuildExecutor`;
  - [ ] watch rebuild loop state.
- [ ] Update CLI help text.
- [ ] Avoid compatibility wrappers for old `build_project(..., flags)` shape unless a test helper cannot be updated in the same slice.

### Tests

- [ ] CLI unit tests for valid repeated inputs.
- [ ] CLI unit tests for duplicate/malformed/missing input values.
- [ ] `build`, `check`, and `dev` parser coverage.
- [ ] Dev-server executor test proving inputs persist across rebuild calls.

### Review / audit / validation

- [ ] Audit for no input data stored in `Flag`.
- [ ] Audit for no long parameter lists where an options struct is clearer.
- [ ] Audit dev rebuild input persistence.
- [ ] Audit no new project tree scan was added for inputs.
- [ ] Run `cargo test cli`, `cargo test check`, `cargo test dev_server`, and `cargo clippy`.
- [ ] Update capsule validation state.

---

## Phase 4 — `#Import of T` syntax, contracts, and resolver core

### Context

This phase teaches the frontend that imported build values are constant declarations with a special source. It should centralize contract validation and value resolution enough for both `config.bst` and ordinary modules to reuse it.

### Checklist

- [ ] Extend declaration syntax to parse `#Import of <type>`.
- [ ] Store constant-source metadata, for example:
  - [ ] fixed constant;
  - [ ] imported constant with inner parsed type.
- [ ] Reject `Import of T` in ordinary runtime type position.
- [ ] Reject `Import` as an ordinary type name unless a real user type with that name is impossible by reservation.
- [ ] Allow missing initializer only for `#Import` declarations.
- [ ] Keep ordinary constants requiring initializers.
- [ ] Header parsing:
  - [ ] record `#Import` shells;
  - [ ] add type dependency edges for inner type;
  - [ ] do not add lowercase import edges;
  - [ ] collect top-level contract candidates;
  - [ ] work with post-hash-root active/imported module root roles;
  - [ ] allow exported `#Import` constants inside `export:` if their final value is a supported compile-time constant.
- [ ] AST environment:
  - [ ] resolve inner type to `TypeId`;
  - [ ] validate supported primitive/optional primitive type;
  - [ ] check default initializer if present;
  - [ ] register constant with semantic type `T`;
  - [ ] record contract name/type/default/location.
- [ ] If body-local `#` constants exist today, support `#Import` there. If not, do not add body-local constants as part of this plan.
- [ ] Add contract unification by name for `#Import` declarations:
  - [ ] same scalar type;
  - [ ] same required/default status;
  - [ ] same primitive default value if present;
  - [ ] source labels for both conflicting sites.
- [ ] Add resolver core:
  - [ ] raw CLI values;
  - [ ] builder globals;
  - [ ] config public globals;
  - [ ] local defaults;
  - [ ] missing required diagnostics.
- [ ] Reuse `numeric_text` for CLI numeric parsing.
- [ ] Keep unknown CLI input validation delayed until reachable source contracts are known.
- [ ] If a fixed config public global has the same name as a source `#Import`, validate type compatibility and use the config value as provider.
- [ ] If a fixed config public global has the same name as a CLI input, emit a fixed-global override diagnostic regardless of source `#Import` contracts.

### Tests

- [ ] `import_constant_default_string`
- [ ] `import_constant_required_missing`
- [ ] `import_constant_bool_default`
- [ ] `import_constant_optional_none_default`
- [ ] `import_constant_export_success`
- [ ] `import_constant_unsupported_type_rejected`
- [ ] `import_constant_conflicting_type_rejected`
- [ ] `import_constant_conflicting_default_rejected`
- [ ] `import_type_keyword_runtime_rejected`
- [ ] `lowercase_import_unchanged`
- [ ] `export_block_import_constant_public`

### Review / audit / validation

- [ ] Audit that `Import` is not represented as a normal semantic type.
- [ ] Audit header dependencies; do not add an AST reorder workaround.
- [ ] Audit `export:` handling for imported constants.
- [ ] Audit diagnostics for structured payloads and stable codes.
- [ ] Run targeted `#Import` tests and `cargo clippy`.
- [ ] Update capsule validation state.

---

## Phase 5 — grouped config, public globals, and end-to-end input resolution

### Context

This is the clean config-shape break. It must land with scaffold/tests so the repository does not sit in a half-flat, half-grouped config state. It must use canonical `config.bst` and the Stage 0 source tree index produced by the prerequisite module plan.

### Checklist

- [ ] Extend `ProjectConfigKeyRegistry` to support:
  - [ ] known top-level primitive config keys;
  - [ ] known top-level record names;
  - [ ] known record fields with owner and `ConfigValueShape`.
- [ ] Keep `ConfigValueShape` broad and simple:
  - [ ] `String`;
  - [ ] `Int`;
  - [ ] `Bool`;
  - [ ] `StringCollection`;
  - [ ] `ClosedStringSet`.
- [ ] Register core top-level keys:
  - [ ] `project` as closed string set currently accepting `"html"`;
  - [ ] `name`;
  - [ ] `version`;
  - [ ] `license`;
  - [ ] `author` only if it remains part of the public metadata surface.
- [ ] Register top-level HTML/public key:
  - [ ] `page_url_style` as public top-level primitive, interpreted by HTML routing config.
- [ ] Register core record fields:
  - [ ] `paths.entry_root`;
  - [ ] `paths.dev_folder`;
  - [ ] `paths.output_folder`;
  - [ ] `paths.package_folders`;
  - [ ] `limits.template_const_loop_iteration_limit`.
- [ ] Register HTML record fields:
  - [ ] `html.origin`;
  - [ ] `html.html_lang`;
  - [ ] `html.html_title_prefix`;
  - [ ] `html.html_title_postfix`;
  - [ ] `html.html_favicon`;
  - [ ] `html.html_inject_charset`;
  - [ ] `html.html_inject_viewport`;
  - [ ] `html.html_inject_color_scheme`;
  - [ ] `html.html_inject_core_css`;
  - [ ] `html.html_body_style`;
  - [ ] `html.redirect_index_html` if this remains grouped rather than top-level.
- [ ] Use canonical dotted key strings for stored settings and locations:
  - [ ] `paths.entry_root`;
  - [ ] `html.html_lang`;
  - [ ] `limits.template_const_loop_iteration_limit`.
- [ ] Update config validation:
  - [ ] process authored top-level constants from `config.bst`;
  - [ ] reject duplicate top-level names;
  - [ ] allow unknown top-level primitive constants as public globals;
  - [ ] apply known top-level primitive config keys to `Config` or `Config.settings`;
  - [ ] store every top-level primitive as a public build global;
  - [ ] record whether each public global is fixed or import-overridable;
  - [ ] require top-level records to be known config records;
  - [ ] validate record fields against the registry;
  - [ ] apply record fields to the same final `Config` shape builders already read;
  - [ ] never expose record values or nested fields as public globals.
- [ ] Update `Config` with a public global map and fixed/import-overridable policy metadata.
- [ ] Update config application helpers:
  - [ ] `paths.entry_root` -> `config.entry_root`;
  - [ ] `paths.dev_folder` -> `config.dev_folder`;
  - [ ] `paths.output_folder` -> `config.release_folder`;
  - [ ] `paths.package_folders` -> `config.package_folders`;
  - [ ] `limits.template_const_loop_iteration_limit` -> `config.template_const_loop_iteration_limit`;
  - [ ] HTML fields -> `config.settings` with canonical names expected by existing HTML parsers, or update HTML parsers to read dotted names consistently.
- [ ] Add migration diagnostics for old flat hidden keys:
  - [ ] `entry_root` -> `paths.entry_root`;
  - [ ] `dev_folder` -> `paths.dev_folder`;
  - [ ] `output_folder` -> `paths.output_folder`;
  - [ ] `package_folders` -> `paths.package_folders`;
  - [ ] `template_const_loop_iteration_limit` -> `limits.template_const_loop_iteration_limit`;
  - [ ] `origin` -> `html.origin`;
  - [ ] `html_lang` -> `html.html_lang`;
  - [ ] `html_title_prefix` -> `html.html_title_prefix`;
  - [ ] `html_title_postfix` -> `html.html_title_postfix`;
  - [ ] `redirect_index_html` -> `html.redirect_index_html`.
- [ ] Do not add compatibility shims that accept old and new shapes together.
- [ ] Integrate resolver behavior:
  - [ ] config `#Import` can use CLI/builder inputs while parsing config;
  - [ ] ordinary source `#Import` can use validated config public globals;
  - [ ] fixed config public globals block same-name CLI input;
  - [ ] source `#Import` with same name as fixed config global must have a compatible type;
  - [ ] unknown CLI input errors after reachable source contracts are known;
  - [ ] conflicting `#Import` contracts error before provided value parsing.
- [ ] Use post-hash-root config discovery. Do not add another filesystem pass to find `config.bst` or source `#Import` declarations.
- [ ] Update `start_page_scaffolding::config_template` to the target grouped config.
- [ ] Update scaffold tests and all integration fixtures containing old flat hidden keys.

### Tests

- [ ] `config_grouped_paths_success`
- [ ] `config_grouped_limits_success`
- [ ] `config_grouped_html_success`
- [ ] `config_top_level_public_global_success`
- [ ] `config_unknown_public_primitive_global_success`
- [ ] `config_unknown_record_rejected`
- [ ] `config_unknown_record_field_rejected`
- [ ] `config_flat_entry_root_rejected`
- [ ] `config_nested_field_not_import_visible`
- [ ] `config_fixed_global_blocks_cli_override`
- [ ] `config_import_global_allows_cli_override`
- [ ] `build_input_cli_overrides_source_import_default`
- [ ] `build_input_config_public_global_used`
- [ ] `build_input_required_missing_rejected`
- [ ] `build_input_invalid_int_rejected`
- [ ] `build_input_invalid_float_rejected`
- [ ] `build_input_invalid_char_rejected`
- [ ] `build_input_invalid_bool_rejected`
- [ ] `build_input_optional_none`
- [ ] `build_input_unknown_cli_rejected`
- [ ] `check_input_success`
- [ ] Dev-server input persistence test if it can be exercised without starting a long-running server.

### Review / audit / validation

- [ ] Audit that config remains Stage 0-owned and `config.bst` is not importable.
- [ ] Audit all builder parsers still consume final `Config`, not AST config records.
- [ ] Audit no old flat hidden config key remains accepted.
- [ ] Audit setting locations point to nested fields where useful.
- [ ] Audit all config/build-input semantic mistakes use `CompilerDiagnostic`.
- [ ] Run `cargo test project_config`, `cargo test new_html_project`, targeted build-input tests, and `cargo run -- tests --backend html`.
- [ ] Update capsule validation state.

---

## Phase 6 — documentation, docs-site migration, roadmap, and matrix

### Context

This phase makes the feature understandable and removes stale public examples. It should happen only after compiler behavior and scaffold shape compile.

### Checklist

- [ ] Update `docs/language-overview.md`:
  - [ ] anonymous record literal syntax;
  - [ ] hidden nominal identity;
  - [ ] field annotations and inference;
  - [ ] exported anonymous const records;
  - [ ] rejected runtime anonymous record escape surfaces;
  - [ ] `#Import of T` syntax;
  - [ ] supported imported types;
  - [ ] repeated contract rule;
  - [ ] config public globals and grouped records;
  - [ ] fixed config constants blocking override;
  - [ ] `config.bst` is the only project config filename.
- [ ] Update `docs/compiler-design-overview.md`:
  - [ ] builder/global input registry;
  - [ ] `BuilderSurface` build-global surface if implemented there;
  - [ ] Stage 0 config/public-global resolution;
  - [ ] source tree index consumption and no extra discovery pass;
  - [ ] unknown CLI input validation timing;
  - [ ] anonymous record `TypeEnvironment` ownership;
  - [ ] final TIR folding/handoff interaction;
  - [ ] HIR/backend rule that anonymous records reuse nominal struct lowering.
- [ ] Update `docs/src/docs/project-structure/#page.bst`:
  - [ ] `config.bst` default config example;
  - [ ] top-level public globals;
  - [ ] grouped hidden builder/internal settings;
  - [ ] note that old flat hidden keys moved;
  - [ ] link to the new imported-build-values page.
- [ ] Add `docs/src/docs/imported-build-values/#page.bst`:
  - [ ] `#Import of T` overview;
  - [ ] CLI examples for `build`, `dev`, and `check`;
  - [ ] required imported values;
  - [ ] defaults;
  - [ ] supported type table;
  - [ ] config public-global interaction;
  - [ ] fixed config override diagnostic example;
  - [ ] duplicate contract rule;
  - [ ] limitations and not-planned syntax.
- [ ] Update docs nav/sidebar/index if the docs site has one.
- [ ] Convert docs-site const-record helper patterns to anonymous records where the named struct only exists to instantiate one const record.
- [ ] Do not convert examples where the named struct is teaching nominal struct syntax.
- [ ] Update examples to use `export:` blocks where public constants are shown.
- [ ] Update `docs/src/docs/progress/#page.bst` rows:
  - [ ] anonymous records;
  - [ ] imported build values;
  - [ ] grouped project config;
  - [ ] deferred/non-planned input syntaxes.
- [ ] Keep `docs/roadmap/roadmap.md` order accurate: this plan after TIR and hash-root/export-block, before HTML Wasm backend.
- [ ] Update README only if visible project config or CLI snippets are stale.

### Documentation examples to include

```beanstalk
version #Import of String = "0.1.0"

#[: Build version: v[version]]
```

```bash
bean build docs --input version=0.1.1 --release
bean dev docs --input version=dev
bean check docs --input version=0.1.1
```

```beanstalk
commit_sha #Import of String
```

```beanstalk
paths #= |
    entry_root = "src",
    dev_folder = "dev",
    output_folder = "release",
    package_folders = {"lib"},
|
```

```beanstalk
export:
    palette #= |
        red String = [$html:<span style="color: red;">[$slot]</span>],
        highlight String = [$html:<span style="color: gold;">[$slot]</span>],
    |
;
```

### Review / audit / validation

- [ ] Audit docs for stale flat config keys.
- [ ] Audit docs for alternate config filename wording.
- [ ] Audit docs for stale `#mod.bst` public API wording where `export:` should be used.
- [ ] Audit docs for unsupported `-D`, `--define`, JSON, aliasing, or direct env syntax.
- [ ] Audit docs for any claim that `config.bst` exports module-visible declarations.
- [ ] Build/check docs site through the normal test path.
- [ ] Run `cargo run -- tests --backend html` and `just validate` if feasible.
- [ ] Update capsule validation state.

---

## Phase 7 — final hardening and cleanup

### Context

This phase removes transitional code, checks boundaries, and proves the feature is stable enough to leave as the new baseline.

### Checklist

- [ ] Search for old flat hidden config examples and either migrate them or make them intentional negative tests:
  - [ ] `entry_root #=`;
  - [ ] `dev_folder #=`;
  - [ ] `output_folder #=`;
  - [ ] `package_folders #=`;
  - [ ] `template_const_loop_iteration_limit #=`;
  - [ ] `origin #=`;
  - [ ] `html_lang #=`;
  - [ ] `html_title_postfix #=`;
  - [ ] `redirect_index_html #=`.
- [ ] Confirm no alternate project config filename or compatibility path remains.
- [ ] Search for old public API wording/examples:
  - [ ] `#mod.bst` public API examples should be migrated where this feature touches them;
  - [ ] inline `export` compatibility should not be reintroduced.
- [ ] Search for unsupported input syntax:
  - [ ] `--define`;
  - [ ] `-D `;
  - [ ] JSON input examples;
  - [ ] key alias examples;
  - [ ] `#Env` or direct env-source syntax.
- [ ] Confirm old deprecated config diagnostics are still sensible:
  - [ ] `libraries`;
  - [ ] `root_folders`;
  - [ ] `src`;
  - [ ] old flat hidden keys moved to records.
- [ ] Confirm diagnostics have stable codes and integration cases assert codes where practical.
- [ ] Confirm no source/config/build-input semantic diagnostics use `CompilerError`.
- [ ] Confirm no user-input parser/AST path panics.
- [ ] Confirm no compatibility wrappers or stale legacy APIs remain.
- [ ] Confirm `bean new html` creates grouped `config.bst`.
- [ ] Confirm docs site compiles with anonymous records.
- [ ] Confirm dev-server rebuilds preserve inputs.
- [ ] Confirm `bean check --input` and `bean build --input` resolve frontend constants identically.
- [ ] Confirm HTML-Wasm either accepts compile-time-only cases or rejects unsupported runtime anonymous record use before backend lowering.
- [ ] Confirm no new source tree/config discovery pass was introduced by this feature.
- [ ] Update active context capsule with last good commit and validation result.

### Full validation

Run:

```bash
just validate
```

Also run targeted commands if not already covered:

```bash
cargo test cli
cargo test project_config
cargo test new_html_project
cargo test dev_server
cargo run -- tests --backend html
cargo run -- tests --backend html_wasm
```

If there are unrelated failures:

- [ ] record exact command;
- [ ] record failure summary;
- [ ] record why unrelated;
- [ ] get explicit acceptance before marking the phase complete.

### Review / audit / validation

- [ ] Manual stage-boundary review:
  - [ ] Stage 0 owns config/build globals and consumes source tree index data.
  - [ ] Header parsing owns declaration discovery, export-block metadata, module-root role handling, and dependency edges.
  - [ ] AST owns semantic type resolution, const folding, final TIR folding/handoff consumption, hidden anonymous type registration, and `#Import` constant resolution.
  - [ ] HIR receives only valid lowered runtime constructs.
  - [ ] Backends consume final `Config` and compiled modules; they do not rediscover config/import/source/template semantics.
- [ ] Manual style-guide review:
  - [ ] modules have clear owners;
  - [ ] named context structs replace noisy long parameter lists;
  - [ ] no obsolete wrappers;
  - [ ] comments explain stage boundaries;
  - [ ] diagnostics are structured;
  - [ ] tests are behavior-focused.
- [ ] Mark plan complete only after validation is accepted.

---

## Coding-agent slice order

1. **Slice 0: Context refresh**
   - Phase 0 only.

2. **Slice 1: Anonymous record syntax and hidden identity**
   - Phase 1 parser, AST shell, type identity, and escape diagnostics.

3. **Slice 2: Anonymous record use**
   - Phase 2 const folding, final TIR interaction, field access, runtime lowering/rejections, and tests.

4. **Slice 3: CLI/build input carrier**
   - Phase 3 raw input model, CLI parser, build/check/dev plumbing.

5. **Slice 4: `#Import` contracts**
   - Phase 4 declaration syntax, contract collection, type validation, resolver core.

6. **Slice 5: Config clean break**
   - Phase 5 grouped config schema, public globals, scaffold, and end-to-end input resolution.

7. **Slice 6: Docs and matrix**
   - Phase 6 compiler docs, docs site, progress matrix, roadmap, and example migration.

8. **Slice 7: Final hardening**
   - Phase 7 cleanup, full validation, and acceptance review.

Each slice must end by refreshing the Active context capsule in this plan file.

---

## Diagnostic inventory

Use exact code names that match the repository’s diagnostic naming style, but cover these cases.

### Anonymous records

- Empty anonymous record.
- Missing anonymous record field initializer.
- Duplicate anonymous record field.
- Anonymous record field type mismatch.
- Anonymous record return rejected.
- Runtime anonymous record exported API exposure rejected.
- Anonymous const record export accepted when fully folded.
- Anonymous record generic escape rejected.
- Anonymous record collection/map use rejected if deferred.

### Imported constants

- `Import` used outside `#` constant declaration type position.
- Missing `of` after `Import`.
- Unsupported imported value type.
- Required imported value missing.
- Provided imported value cannot parse as declared type.
- Conflicting imported value contract.
- Imported default is not const.
- Imported default has wrong type.
- Ordinary constant missing initializer.
- Source `#Import` type incompatible with same-name fixed config global.

### CLI/build inputs

- Duplicate CLI input.
- Missing `--input` value.
- Invalid `--input` name.
- Malformed `name=value` pair.
- Unknown CLI input.
- CLI input attempts to override fixed config global.
- CLI input type mismatch.

### Config records/public globals

- Old flat key moved to record.
- Unknown config record.
- Unknown config record field.
- Expected record value for known config record.
- Invalid record field shape.
- Unsupported public global type.
- Nested field not importable.

---

## Test matrix

### Unit tests

- CLI parsing and duplicate/malformed input handling.
- Build scalar parser for every supported type and optional shape.
- Config registry top-level and record-field lookup.
- Config validation application into `Config` and public global map.
- Hidden anonymous record type identity.
- Diagnostic rendering for hidden anonymous type names.
- Final TIR-backed template field folding where a unit-level check is available.

### Integration tests

Suggested cases:

```text
anonymous_record_local_field_access
anonymous_record_typed_fields
anonymous_const_record_projection
anonymous_const_record_template_fields
anonymous_const_record_export_success
anonymous_record_duplicate_field_rejected
anonymous_record_empty_rejected
anonymous_record_shape_not_structural
anonymous_record_return_rejected
anonymous_record_exported_signature_rejected
anonymous_record_exported_field_type_rejected

config_grouped_paths_success
config_grouped_limits_success
config_grouped_html_success
config_top_level_public_global_success
config_unknown_public_primitive_global_success
config_unknown_record_rejected
config_unknown_record_field_rejected
config_flat_entry_root_rejected
config_nested_field_not_import_visible
config_fixed_global_blocks_cli_override
config_import_global_allows_cli_override

import_constant_default_string
import_constant_required_missing
import_constant_cli_override_string
import_constant_cli_override_bool
import_constant_optional_none
import_constant_invalid_int_rejected
import_constant_invalid_float_rejected
import_constant_invalid_char_rejected
import_constant_invalid_bool_rejected
import_constant_unsupported_type_rejected
import_constant_conflicting_type_rejected
import_constant_conflicting_default_rejected
import_constant_unknown_cli_rejected
import_constant_export_success
```

### Backend coverage

- HTML:
  - success and diagnostics for the full V1 surface.
- HTML-Wasm:
  - compile-time/config cases where no unsupported runtime feature is reachable;
  - runtime anonymous record cases only if existing struct lowering supports them, otherwise target-contract diagnostics before lowering.

---

## Roadmap and matrix updates

### `docs/roadmap/roadmap.md`

Keep this plan under `# Plans` after TIR finalisation and after the hash-root/export-block module-system plan, before the HTML project builder Wasm backend plan:

```markdown
- [#Import values and anonymous records plan](docs/roadmap/plans/import_values_anonymous_records_plan.md)
```

### `docs/src/docs/progress/#page.bst`

Add/update rows for:

- Anonymous record literals:
  - V1: hidden nominal type per literal site, field access, const records, exported folded const records, no structural typing, no returns/runtime public API exposure.
- Imported build values:
  - V1: `#Import of T`, primitive/optional primitives only, repeated `--input`, no aliasing/JSON/`-D`/`--define`.
- Project config:
  - `config.bst` public top-level primitive globals plus grouped records; old flat hidden keys moved.
- Module/export interaction:
  - public const records can live in `export:` blocks; runtime anonymous record types cannot be exposed.
- Deferred/future:
  - Beanstalk-native env-file/general input source;
  - future numeric widths as imported types;
  - non-primitive imported values only if a future design accepts them;
  - anonymous record collections/generic propagation if rejected in V1.
- Rejected/not planned:
  - key aliasing;
  - JSON inputs;
  - `-D`;
  - `--define`;
  - direct source-level OS env lookup;
  - structural typing.

---

## Deliberately deferred or rejected features

### Deferred after V1

- Beanstalk-native env-file/general input source, likely through config or a companion special file.
- Future numeric widths as imported value types after the numeric type plan lands.
- Non-primitive imported values only if a future design avoids general value serialization.
- Anonymous records in collections/hashmaps if rejected in V1.
- Anonymous record generic propagation if rejected in V1.
- Additional docs-site anonymous-record migrations not completed in Phase 6.

### Rejected / not planned

- Key aliasing for `#Import`.
- `-D`.
- `--define`.
- JSON build inputs.
- Direct source-level OS environment lookup such as `#Env`.
- Structural typing.
- Shape-based anonymous record unification.
- Returning anonymous records.
- Runtime anonymous record public API exposure.
- Receiver methods on anonymous records.
- Trait conformance for anonymous records.
- Making `Import` a runtime wrapper type.
- Making lowercase `import` read build inputs.
- Making `config.bst` importable as a module.
