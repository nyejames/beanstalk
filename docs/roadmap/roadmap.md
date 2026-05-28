# Beanstalk Roadmap
This is the main todo list for the language and compiler.

The current major goal is getting to a healthy alpha stage.
Each plan or PR that is needed will be linked here.

Use the language surface integration matrix as a reference for what is currently implemented: `docs/src/docs/progress/#page.bst`

AST optimisation benchmark log: `docs/roadmap/refactors/ast-pipeline-optimisation-benchmark-log.md`

---

# Plans / TODOS
- Assert implementation as the only panic surface: `docs/roadmap/plans/assert_panic_implementation_plan.md`
- Basic template control flow statements (if expressions, loops and if statements)
- Traits
- Generic follow-ups after cross-module free-function support:
  - Decide explicit generic call syntax. Current rejected examples include `identity of Int(42)`, `identity[Int](42)`, and `identity<Int>(42)`.
  - Design generic function values and higher-order polymorphism.
  - Design generic receiver methods and receiver methods on instantiated generic types.
  - Add trait bounds / constraints before broadening behavior-dependent operations on generic parameters.
  - Design direct facade re-export syntax, function alias exports, and automatic method/function re-export through facade aliases.
  - Design generic external JS/package functions and backend glue.
  - Add richer generic instantiation diagnostics with substitution/declaration context beyond the current call-site/body labels.
  - Design generic aliases with parameters and partial type application.
- `else => _` (Wildcards in value positions for pattern matching or default arguments in function calls)
- Replace JSON with beanstalk files (dogfooding for language as a way to store data / config stuff). These could be standardised as their own build system under `src/projects`.
- Closures
- Wasm backend plan based on docs inside `docs/wasm-notes`
- Hash Maps (core library)
- Collection capacity type extension `{Int 64}`
- Possibly change generics syntax from `type T` to more conventional syntax? `<T>`
- Compile time arbitary precision aritmetic + Decimals Type support
- Move to more specific explicit type declarations for numbers (I32, I64, F32, F64) - JS backend just makes all an F64 and accepts the precision loss, more for future Wasm backend

# Notes
- External non-scalar constant design: string slices, collections, and opaque-type external constants in const contexts are rejected for Alpha. Design compile-time representation and validation before enabling.
- Private const/config follow-ups after the private const config refactor: consume HIR const metadata in borrow checking, temporary-local reduction, and lowering/constant propagation.
- Typed config follow-ups after the private const config refactor: structured typed config values with choices/const records, future `project = Project::Html(...)` syntax, typed backend config schemas, optional config-local helper constants, config lock/cache metadata, numeric config shapes when keys need them, and private inferred `=` const-record config projection.
- `bean new` follow-ups: non-interactive `--default`, template selection, project type aliases, richer scaffold presets, and optional package/dev tooling setup.
- In the current architecture, source libraries are compiled into each consuming module. A future package system may move to separate library compilation, where libraries are built first and project modules consume pre-compiled library artifacts.

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
