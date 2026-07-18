# Beanstalk Roadmap
This is the main todo list and future design / implementation roadmap for Beanstalk.

The next major plans are kept inside [plans](docs/roadmap/plans) and linked here in top to bottom order under the `Plans` heading.

Use the [Progress Matrix](docs/src/docs/progress/#page.bst) as a reference for what is currently implemented, partially complete or deferred.

---

# Plans

## Active implementation work

- [Final TIR completion](docs/roadmap/plans/final-tir-completion-plan.md)
- [Compiler diagnostics improvements](docs/roadmap/plans/compiler-diagnostics-improvement-plan.md)

## Queued implementation chain

- [Canonical module compilation and scoped packages](docs/roadmap/plans/canonical-module-compilation-and-scoped-packages-plan.md)
- [Project config, imported build values and anonymous records](docs/roadmap/plans/import_values_anonymous_records_plan.md)
- [Entry-local config blocks and runtime title](docs/roadmap/plans/entry-config-blocks-runtime-title-plan.md)
- [Number and numeric semantics](docs/roadmap/plans/number_type_numeric_plan.md)
- [HTML mixed JavaScript and Wasm backend](docs/roadmap/plans/html_project_backend_wasm_final_implementation_plan.md)

Diagnostics may continue independently. The queued implementation chain remains ordered by hard dependency.

Do not mark a plan active unless its current-state capsule says it is active.

---

# Deferred design and follow-ups

These items are genuinely deferred. They are not current implementation work. Each item links to its owning plan or stays here only when no plan exists yet.

## Post-TIR template performance follow-ups

The final TIR architecture creates safe extension hooks, but the actual optimisations below are deferred until profiling or broader compiler infrastructure justifies them:

- source-span-backed template body text instead of eager `StringId` interning
- per-template parse cache
- formatter-output cache
- dev-mode source-hash keyed template reuse
- dependency-aware invalidation for imported consts and directives
- formatter algorithm rewrites only if post-TIR profiling justifies them
- incremental module and template compilation after module-boundary incremental builds exist
- parallel nested-template folding after a separate profiling-backed plan

See the [final TIR completion plan](docs/roadmap/plans/final-tir-completion-plan.md) for TIR ownership and the [frontend optimisation plan](docs/roadmap/plans/frontend-arena-semantic-invariant-optimization-plan.md) for arena and invariant work.

## Genuinely deferred items

- final builder selection syntax and a possible Beanstalk-native build script system
- package declaration syntax, registries, remote fetching, version solving and lockfiles
- persistent artefact serialisation and precompiled package caches
- explicit output transformation pipeline syntax
- cross-page browser chunk sharing beyond physical variant reuse
- direct normal-sibling imports if real project evidence justifies them
- broader reactivity source design
- additional target builders and capability surfaces
- profiling-backed frontend or TIR optimisations
- future Component Model integration
- incremental builds at the module boundary after persistent artefact serialisation exists
- ownership optimisation deferred until after GC-first correctness
- external non-scalar constant design: string slices, collections and opaque-type external constants in const contexts
- private const and config follow-ups: consume HIR const metadata in borrow checking, temporary-local reduction and constant propagation
- `bean new` follow-ups: non-interactive `--default`, template selection, project type aliases, richer scaffold presets and optional package or dev tooling setup
- benchmarking and profiling deferred tooling: CI performance gates, public dashboards, source-backed package HIR caching, ownership, drop and ABI specialisation, JS minification and tree shaking, package-manager caching, broad Criterion benchmark suites, tracing and allocation profiler integrations, and tracked-summary counter expansion

## Reactivity follow-ups

After the initial reactivity surface:

- reactive template control flow
- field and path subscriptions
- collection item subscriptions
- expression dependency tracking
- derived reactive values
- template-owned event, action and effect syntax
- `$bind(...)`
- typed component messages
- IO sink design
- fine-grained DOM updates
- nested reactive regions
- keyed loop diffing
- HTML-Wasm support

## Hash map follow-ups

After the current scalar-keyed builtin map surface:

- Wasm runtime and lowering for the existing scalar-keyed builtin map
- possible read-only map iteration only if it does not introduce `HASHABLE`, custom equality, custom hashers, mutable entry APIs or user-defined key semantics

## Collection follow-ups

After fixed collection type constraints:

- default-fill syntax such as `{...none}` and `{...0}`
- explicit fixed and growable conversion through `copy` after cast and copy hardening
- growable initial-capacity hints only if future backend work shows they are useful

## Trait ecosystem follow-ups

After the initial trait surface:

- static non-method requirements
- compiler-owned builtin conformance facts
- diagnostics and tooling polish
- broader standard trait taxonomy that keeps traits as static contracts only

## Time package follow-ups

After the first `@core/time` JavaScript slice:

- civil and calendar types (`Date`, `TimeOfDay`, `DateTime`, `TimeZone`, `ZonedDateTime`, `Period`)
- Temporal-backed JS calendar behaviour once runtime and polyfill policy is clear
- locale-aware formatting and parsing
- local time-zone lookup
- async timers, sleep and intervals after async and task design exists
- browser animation-frame integration in a web-specific package rather than `@core/time`
- Wasm and native lowerings
- higher-precision or nanosecond timestamp representation if wider numeric ABI work lands

## Deferred package-system follow-ups

After the canvas reachability refactor:

- JS-backed external package APIs
- Wasm implementations for JS-backed packages such as `@web/canvas`
- current reachability is artefact-planning correctness, not general JS tree shaking or minification

---

# Outside Language Design Scope

These surfaces are intentionally not roadmap items unless the language philosophy is explicitly changed first:

- Dynamic trait values, trait objects, dynamic trait runtime lowering, trait aliases and composition, downcasting and reflection, associated types and constants, inheritance, generic traits and methods, and blanket, conditional, negative or specialized conformance.
- `HASHABLE`, generic builtin map keys, user-defined builtin map keys, custom hashers and comparers, `Float` map keys, language-level map equality, mutable entry APIs, fixed or capacity maps, and language hashsets.
- First-class public `Result` values, exceptions, reflection and runtime type IDs, broad type-level programming, higher-kinded types, parameterized aliases, partial type application, and general macro systems.
- User-defined cast targets, generic cast targets, external opaque cast targets, generic cast traits, and broad return-type-directed conversion.
- General closures, anonymous function values, generic function values, and higher-order polymorphism. Reactivity is the constrained UI-oriented mechanism intended to cover many closure-heavy UI patterns without adding general function-value semantics.

---

# Future Design Notes

## Package manager ideas

- Should try to prevent dependency explosion as much as possible, make adding dependencies with lots of dependencies harder or discouraged.
- Idea of "Golden" packages (and silver, bronze etc):
    1. Golden dependencies have 0 dependencies themselves (outside of std or core)
    2. Silver dependencies only have golden dependencies
    3. Bronze dependencies only have silver or gold dependencies
    4. Lead dependencies do not meet these criteria and there is additional friction and checks before they can be added to a project.
- Lead dependencies may not be eligible for the future official Beanstalk package registry and will not be supported automatically by the package manager.
- The package manager should be extremely strict about security and other things before something can become an official "package". Maybe the source code must pass a series of quality checks and be run through various bits of compiler tooling before it can be added.
