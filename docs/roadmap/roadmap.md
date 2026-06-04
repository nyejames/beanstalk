# Beanstalk Roadmap
This is the main todo list for the language and compiler.

The current major goal is getting to a healthy alpha stage.
Each plan or PR that is needed will be linked here.

Use the language surface integration matrix as a reference for what is currently implemented: `docs/src/docs/progress/#page.bst`

AST optimisation benchmark log: `docs/roadmap/refactors/ast-pipeline-optimisation-benchmark-log.md`

---

# Plans / TODOS
- `.bmd` files "beandown", a bit like markdown files. (starts inside template body and cannot break out of template). `docs/roadmap/plans/beandown-implementation-plan.md`. Intended to be simple. Can be imported into regular beanstalk files as compile time strings.
- Collection capacity type extension `{64 Int}`
- Build out core math library
- Replace JSON with beanstalk `.struct` files (dogfooding for language as a way to store data / config stuff). These could be standardised as their own build system under `src/projects`.
- Closures
- Wasm backend plan based on docs inside `docs/wasm-notes`
- Hash Maps (core library)
- Compile time arbitary precision aritmetic + Decimals Type support
- Move to more specific explicit type declarations for numbers (I32, I64, F32, F64).JS backend just makes all an F64 and accepts the precision loss, more for future Wasm backend

# Notes
- Trait ecosystem follow-ups after Traits v1: default methods, associated types/constants,
  static non-method requirements, trait inheritance/composition, generic traits/methods,
  conditional and specialized generic instance conformances, dynamic trait composition,
  aliases/downcasting/reflection, file-local evidence-backed generic bound dispatch,
  compiler-owned builtin conformances, `DISPLAYABLE` output coercion, operator/boolean keyword
  integration, broader standard trait taxonomy, automatic primitive conformances, and Wasm
  dynamic trait lowering.

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
