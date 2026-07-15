# Beanstalk Roadmap
This is the main todo list and future design / implementation roadmap for Beanstalk.

The next major plans are kept inside [plans](docs/roadmap/plans) and linked here in top to bottom order under the `Plans` heading.

Use the [Progress Matrix](docs/src/docs/progress/#page.bst) as a reference for what is currently implemented, partially complete or deferred.

---

# Plans
- [Some cleanup](docs/roadmap/plans/codebase-integrity-cleanup-plan.md)
- [TIR Finalisation plan](docs/roadmap/plans/final-tir-completion-plan.md)
- [Diagnostics Improvements](docs/roadmap/plans/compiler-diagnostics-improvement-plan.md)
- [Config Blocks replacing top level project builder keys](docs/roadmap/plans/entry-config-blocks-runtime-title-plan.md)
- [New Number type for precise numerical values](docs/roadmap/plans/number_type_numeric_plan.md)
- [#Import values and anonymous records plan](docs/roadmap/plans/import_values_anonymous_records_plan.md)
- [HTML project builder Wasm backend plan](docs/roadmap/plans/html_project_backend_wasm_final_implementation_plan.md)

# Post-TIR template performance follow-ups (deferred)

The final TIR architecture creates safe extension hooks, but the actual optimisations below are deferred until profiling or broader compiler infrastructure justifies them:

- source-span-backed template body text instead of eager `StringId` interning;
- per-template parse cache;
- formatter-output cache;
- dev-mode source-hash keyed template reuse;
- dependency-aware invalidation for imported consts/directives;
- formatter algorithm rewrites only if post-TIR profiling justifies them;
- incremental module/template compilation after module-boundary incremental builds exist;
- parallel nested-template folding after a separate profiling-backed plan.

# Follow up notes and possible TODOs for future plans

- [Optimisation plan](docs/roadmap/plans/frontend-arena-semantic-invariant-optimization-plan.md):
  template churn/capacity/clone-reduction work has been split out into the active
  [`template-optimisation-and-tir-implementation-plan.md`](docs/roadmap/plans/template-optimisation-and-tir-implementation-plan.md).
  Broad template-to-TIR arena migration remains deferred until Plan A measurement and Plan B
  scaffolding justify it.

- Keep ownership optimization deferred: preserve `DropIfOwned` / `Release` hooks, but make v1 correctness GC/handle-first.

- Decide when dispatcher-loop CFG is acceptable permanently and when to add structured CFG lowering as an optimization pass.

- Add a follow-up plan for future Component Model / Wasm module-system integration after core module ABI and external package semantics are stable.

- incremental builds at the module boundary. `dev` when first launched performs a full dev build of the project, then any rebuilds only incrementally build from there based on which modules are actually changed.

- Reactivity follow-ups after V1: reactive template control flow, field/path subscriptions,
  collection item subscriptions, expression dependency tracking, derived reactive values,
  template-owned event/action/effect syntax, `$bind(...)`, typed component messages, IO sink
  design, fine-grained DOM updates, nested reactive regions, keyed loop diffing, and HTML-Wasm
  support.

- Hash map follow-ups after V1: Wasm runtime/lowering for the existing scalar-keyed builtin map
  surface and possible read-only map iteration only if it does not introduce `HASHABLE`, custom
  equality, custom hashers, mutable entry APIs, or user-defined key semantics.

- Collection follow-ups after fixed collection type constraints: default-fill syntax such as
  `{...none}` / `{...0}`, explicit fixed/growable conversion through `copy` after cast/copy
  hardening, and growable initial-capacity hints only if future backend work shows they are useful.

- Trait ecosystem follow-ups after Traits v1: static non-method requirements, compiler-owned
  builtin conformance facts, diagnostics/tooling polish, and broader standard trait taxonomy that
  keeps traits as static contracts only.

- Time package follow-ups after the first `@core/time` JS slice: civil/calendar types
  (`Date`, `TimeOfDay`, `DateTime`, `TimeZone`, `ZonedDateTime`, `Period`),
  Temporal-backed JS calendar behavior once runtime/polyfill policy is clear,
  locale-aware formatting/parsing, local time-zone lookup, async timers/sleep/intervals
  after async/task design exists, browser animation-frame integration in a web-specific
  package rather than `@core/time`, Wasm/native lowerings, and higher-precision or
  nanosecond timestamp representation if wider numeric ABI work lands.

- Deliberately deferred package-system follow-ups after the canvas reachability refactor:
  JS-backed external package APIs, and Wasm implementations for JS-backed packages such as
  `@web/canvas`. Current reachability is artifact-planning correctness, not general JS
  tree-shaking/minification.

- External non-scalar constant design: string slices, collections, and opaque-type external constants in const contexts are rejected for Alpha. Design compile-time representation and validation before enabling.

- Private const/config follow-ups after the private const config refactor: consume HIR const metadata in borrow checking, temporary-local reduction, and lowering/constant propagation.

- Typed config follow-ups after the private const config refactor: structured typed config values with choices/const records, future `project = Project::Html(...)` syntax, typed backend config schemas, optional config-local helper constants, config lock/cache metadata, numeric config shapes when keys need them, and private inferred `=` const-record config projection.

- `bean new` follow-ups: non-interactive `--default`, template selection, project type aliases, richer scaffold presets, and optional package/dev tooling setup.

- Benchmarking/profiling deferred tooling: CI performance gates, public dashboards,
  source-backed package HIR caching, ownership/drop/ABI specialization, JS minification/tree-shaking,
  package-manager caching, broad Criterion benchmark suites, tracing/allocation profiler
  integrations, and tracked-summary counter expansion remain outside the current benchmarking
  implementation. These tools should be added only when they answer a specific optimization
  question and should not become part of the default validation path.

---

# Outside Language Design Scope

These surfaces are intentionally not roadmap items unless the language philosophy is explicitly
changed first:

- Dynamic trait values / trait objects, dynamic trait runtime lowering, trait aliases/composition,
  downcasting/reflection, associated types/constants, inheritance, generic traits/methods, and
  blanket/conditional/negative/specialized conformance.
- `HASHABLE`, generic builtin map keys, user-defined builtin map keys, custom hashers/comparers,
  `Float` map keys, language-level map equality, mutable entry APIs, fixed/capacity maps, and
  language hashsets.
- First-class public `Result` values, exceptions, reflection/runtime type IDs, broad type-level
  programming, higher-kinded types, parameterized aliases, partial type application, and general
  macro systems.
- User-defined cast targets, generic cast targets, external opaque cast targets, generic cast
  traits, and broad return-type-directed conversion.
- General closures, anonymous function values, generic function values, and higher-order
  polymorphism. Reactivity is the constrained UI-oriented mechanism intended to cover many
  closure-heavy UI patterns without adding general function-value semantics.

---

# Future Design Notes

## Wasm
- Define the Wasm external package policy: host imports, JS-backed package rejection, Core package native lowerings and future package-provided Wasm imports.
- Add Wasm lowerings for selected core packages in order: `@core/math`, `@core/text`, `@core/random`, then `@core/time`.
- Split HTML-Wasm integration from generic Wasm module output so browser bootstrap policy does not leak into the core backend.
- Add a Wasm capability matrix tracking scalar operations, strings/templates, structs, choices, options/results, collections, generics, traits, binding-backed packages, Core packages, assertions, IO and runtime memory helpers.
- Harden reachable unsupported-backend diagnostics so every unsupported Wasm feature fails before HIR-to-LIR lowering or byte emission.
- Stabilize the HIR-to-Wasm-LIR contract and document which HIR constructs are accepted, rejected, or lowered through runtime helpers.
- Define the Wasm ABI type mapping for scalars, handles, strings, collections, structs, choices, options, and errors.
- Complete the runtime string model: allocation, UTF-8 layout, interpolation helpers, host string extraction, release hooks, and replacement of bridge-only helpers.
- Design and implement Wasm layout for structs, including field offsets, alignment, construction, field access, mutation, and ownership hooks.
- Design and implement Wasm layout for choices, including unit variants, payload variants, tag representation, payload storage, equality, matching, and generic choices.
- Design and implement Wasm lowering for options, fallible results, multi-return carriers, `catch`, postfix `!`, postfix `?`, and error payload propagation.
- Add Wasm validation and artifact assertions to canonical integration cases, using backend-specific `expect.toml` sections and `golden/html_wasm/` outputs.
- rt_string_from_i64 Wasm helper: Explicitly noted in the 1ac2613 commit message as an "incremental bridge implementation". It produces valid output but is not a complete runtime implementation. This is scoped for a dedicated follow-up and does not cause panics.

## Package manager ideas
- Should try to prevent dependency explosion as much as possible, make adding dependencies with lots of dependencies harder / discouraged
- Idea of "Golden" packages (and silver / bronze etc):
    1. Golden dependencies have 0 depedencies themselves (outside of std or core)
    2. Silver dependencies only have golden dependencies
    3. Bronze dependencies only have silver or gold dependencies
    4. Lead dependencies don't meet these criteria and there is additional friction and checks before they can be added to a project.
Lead dependencies may not be eligible for the future official Beanstalk package registry and won't be supported automatically by the package manager.

The package manager should be extremely strict about security and other things before something can become an official "package".
Maybe the source code must pass a series of quality checks and be ran through various bits of compiler tooling before it can be added.

In the current architecture, source-backed packages are compiled into each consuming module. A future package system may move to separate package compilation, where packages are built first and project modules consume precompiled package artifacts.
