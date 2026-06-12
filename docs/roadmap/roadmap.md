# Beanstalk Roadmap
This is the main todo list for the language and compiler.

The current major goal is getting to a healthy alpha stage.
Each plan or PR that is needed will be linked here.

Use the language surface integration matrix as a reference for what is currently implemented: `docs/src/docs/progress/#page.bst`

AST optimisation benchmark log: `docs/roadmap/refactors/ast-pipeline-optimisation-benchmark-log.md`

---

# Plans / TODOS
- `cast` keyword for builtin type casting sugar for builtin Beanstalk types: `docs/roadmap/plans/cast_operator_implementation_plan.md`
- first class Reactivity syntax with message / action patterns in templates
- Build out core IO library
- Compile time arbitary precision aritmetic + Decimals Type support
- Write a Wasm backend design baseline covering the v1 target, explicit deferred features, ABI/layout rules, runtime helper contracts, and HTML-Wasm bootstrap contract.
- Freeze Wasm v1 as core Wasm, linear-memory handles, single-result ABI, dispatcher-loop CFG, JS/host bootstrap, no Wasm GC, no reference types, no Component Model.
- Add a Wasm capability matrix tracking scalar operations, strings/templates, structs, choices, options/results, collections, generics, traits, external packages, core libraries, assertions, IO, and runtime memory helpers.
- Harden reachable unsupported-backend diagnostics so every unsupported Wasm feature fails before HIR-to-LIR lowering or byte emission.
- Stabilize the HIR-to-Wasm-LIR contract and document which HIR constructs are accepted, rejected, or lowered through runtime helpers.
- Define the Wasm ABI type mapping for scalars, handles, strings, collections, structs, choices, options, and errors.
- Complete the runtime string model: allocation, UTF-8 layout, interpolation helpers, host string extraction, release hooks, and replacement of bridge-only helpers.
- Design and implement Wasm layout for structs, including field offsets, alignment, construction, field access, mutation, and ownership hooks.
- Design and implement Wasm layout for choices, including unit variants, payload variants, tag representation, payload storage, equality, matching, and generic choices.
- Design and implement Wasm lowering for options, fallible results, multi-return carriers, `catch`, postfix `!`, postfix `?`, and error payload propagation.
- Decide the v1 generic runtime policy: monomorphized concrete layouts, explicit rejection boundaries, or a handle-erased fallback for specific cases.
- Define the Wasm external package policy: host imports, JS-backed package rejection, core library native lowerings, and future package-provided Wasm imports.
- Add Wasm lowerings for selected core packages in order: `@core/math`, `@core/text`, `@core/random`, then `@core/time`.
- Split HTML-Wasm integration from generic Wasm module output so browser bootstrap policy does not leak into the core backend.
- Keep ownership optimization deferred: preserve `DropIfOwned` / `Release` hooks, but make v1 correctness GC/handle-first.
- Add Wasm validation and artifact assertions to canonical integration cases, using backend-specific `expect.toml` sections and `golden/html_wasm/` outputs.
- Decide when dispatcher-loop CFG is acceptable permanently and when to add structured CFG lowering as an optimization pass.
- Add a follow-up plan for future Component Model / Wasm module-system integration after core module ABI and external package semantics are stable.


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

# Notes
- Language surface hardening follow-up is complete in `docs/roadmap/plans/hardening_followup_plan.md`: stale dynamic-trait/extension/fallback wording was removed, receiver-method visibility was simplified, concrete trait-evidence receiver fallback was removed, fixed-capacity and receiver coverage was hardened, map-key ownership was documented, and final stale-system audit plus validation passed.

- Hash Maps V1 is complete in `docs/roadmap/plans/hashmaps-implementation-plan.md`: first-class insertion-ordered hashmaps with `{Key = Value}` type syntax, `{key = value}` literals, frontend/HIR/borrow validation, HTML JavaScript support, and HTML-Wasm unsupported-feature diagnostics.

- Hash map follow-ups after V1: Wasm runtime/lowering for the existing scalar-keyed builtin map
  surface and possible read-only map iteration only if it does not introduce `HASHABLE`, custom
  equality, custom hashers, mutable entry APIs, or user-defined key semantics.

- Collection follow-ups after fixed collection type constraints: default-fill syntax such as
  `{...none}` / `{...0}`, explicit fixed/growable conversion through `copy` after cast/copy
  hardening, and growable initial-capacity hints only if future backend work shows they are useful.

- Trait ecosystem follow-ups after Traits v1: static non-method requirements, compiler-owned
  builtin conformance facts, diagnostics/tooling polish, and broader standard trait taxonomy that
  keeps traits as static contracts only.

- Time library follow-ups after the first `@core/time` JS slice: civil/calendar types
  (`Date`, `TimeOfDay`, `DateTime`, `TimeZone`, `ZonedDateTime`, `Period`),
  Temporal-backed JS calendar behavior once runtime/polyfill policy is clear,
  locale-aware formatting/parsing, local time-zone lookup, async timers/sleep/intervals
  after async/task design exists, browser animation-frame integration in a web-specific
  package rather than `@core/time`, Wasm/native lowerings, and higher-precision or
  nanosecond timestamp representation if wider numeric ABI work lands.

- Deliberately deferred library-system follow-ups after the canvas reachability refactor:
  JS-backed external package APIs, and Wasm implementations for JS-backed packages such as
  `@web/canvas`. Current reachability is artifact-planning correctness, not general JS
  tree-shaking/minification.

- External non-scalar constant design: string slices, collections, and opaque-type external constants in const contexts are rejected for Alpha. Design compile-time representation and validation before enabling.

- Private const/config follow-ups after the private const config refactor: consume HIR const metadata in borrow checking, temporary-local reduction, and lowering/constant propagation.

- Typed config follow-ups after the private const config refactor: structured typed config values with choices/const records, future `project = Project::Html(...)` syntax, typed backend config schemas, optional config-local helper constants, config lock/cache metadata, numeric config shapes when keys need them, and private inferred `=` const-record config projection.

- `bean new` follow-ups: non-interactive `--default`, template selection, project type aliases, richer scaffold presets, and optional package/dev tooling setup.

- In the current architecture, source libraries are compiled into each consuming module. A future package system may move to separate library compilation, where libraries are built first and project modules consume pre-compiled library artifacts.

- Benchmarking/profiling deferred tooling: CI performance gates, public dashboards,
  source-library HIR caching, ownership/drop/ABI specialization, JS minification/tree-shaking,
  package-manager caching, broad Criterion benchmark suites, tracing/allocation profiler
  integrations, and tracked-summary counter expansion remain outside the current benchmarking
  implementation. These tools should be added only when they answer a specific optimization
  question and should not become part of the default validation path.

## Wasm

Broader Wasm maturity beyond the current experimental path.

## Package manager ideas
- Should try to prevent dependency explosion as much as possible, make adding dependencies with lots of dependencies harder / discouraged
- Idea of "Golden" libraries (and silver / bronze etc):
    1. Golden dependencies have 0 depedencies themselves (outside of std or core)
    2. Silver dependencies only have golden dependencies
    3. Bronze dependencies only have silver or gold dependencies
    4. Lead dependencies don't meet these criteria and there is additional friction and checks before they can be added to a project.
Lead dependencies maybe won't even be allowed to be uploaded to the official Beanstalk libraries / docs website (a future site that will be very similar to crates.io) and so won't be supported automatically by the package manager. 

The package manager should be extremely strict about security and other things before something can become an official "package".
Maybe the source code must pass a series of quality checks and be ran through various bits of compiler tooling before it can be added.

### Notes and limitations from previous investigations
- The WASM backend can't handle Choice/Union types yet (maps to Handle but produces i32/i64 mismatches). 
- Exponents (requires explicit imported core math support)
- rt_string_from_i64 Wasm helper: Explicitly noted in the 1ac2613 commit message as an "incremental bridge implementation". It produces valid output but is not a complete runtime implementation. This is scoped for a dedicated follow-up and does not cause panics.
