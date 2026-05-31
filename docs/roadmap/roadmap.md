# Beanstalk Roadmap
This is the main todo list for the language and compiler.

The current major goal is getting to a healthy alpha stage.
Each plan or PR that is needed will be linked here.

Use the language surface integration matrix as a reference for what is currently implemented: `docs/src/docs/progress/#page.bst`

AST optimisation benchmark log: `docs/roadmap/refactors/ast-pipeline-optimisation-benchmark-log.md`

---

# Plans / TODOS
- assert termination: `docs/roadmap/plans/assert-terminality-implementation-plan.md`
- Traits
- `else => _` (Wildcards in value positions for pattern matching or default arguments in function calls)
- Replace JSON with beanstalk files (dogfooding for language as a way to store data / config stuff). These could be standardised as their own build system under `src/projects`.
- Closures
- Wasm backend plan based on docs inside `docs/wasm-notes`
- Hash Maps (core library)
- Collection capacity type extension `{Int 64}`
- Compile time arbitary precision aritmetic + Decimals Type support
- Move to more specific explicit type declarations for numbers (I32, I64, F32, F64) - JS backend just makes all an F64 and accepts the precision loss, more for future Wasm backend

# Notes
- Final generics design and implementation is complete. The accepted implementation record is
  `docs/roadmap/plans/generics-hardening-implementation-plan.md`.

- The template control-flow runtime-slot refactor is complete. Template head suffix control flow
  is implemented for source-authored Bool `if`, option-present `if`, range `loop`, collection
  `loop`, standalone `[else]`, standalone `[else if ...]`, structural `[break]` / `[continue]`,
  const folding where supported, lazy runtime HIR lowering, and runtime slot applications with
  branch/loop contributions plus `$children(...)` / `$fresh` wrapper behavior.

- Assert/panic follow-ups after the always-checked `assert` implementation: debug-only assertions,
  lazy runtime assertion-message expressions, compile-time constant assertion messages,
  catchable/recoverable panic design, explicit stop helpers such as `todo` / `unreachable` /
  `fatal` / `abort` / `precondition`, Wasm trap message support, and richer runtime failure
  metadata or stack traces.

- The canvas helper import/runtime reachability refactor is complete. Grouped virtual external
  package imports resolve before source/module facade enforcement without weakening real source
  facade privacy. HTML JS runtime assets, generated glue, runtime modules, import maps, and
  unsupported-backend validation are driven by HIR calls reachable from the entry `start` function.
  Implementation record: `docs/roadmap/plans/canvas-helper-import-runtime-refactor-plan.md`.

- Deliberately deferred library-system follow-ups after the canvas reachability refactor: direct
  facade re-export syntax, wildcard imports, automatic re-export of receiver methods through
  facade type aliases, source-library HIR caching, user-authored external binding files, broader
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
