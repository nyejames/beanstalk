# Beanstalk Structure Steering Document

## Purpose
- Keep the compiler, build system and CLI aligned as the repo grows; use this with the Codebase Style Guide to avoid structural drift.
- Give new contributors a fast map of the moving pieces before they add or reorganise code.

## Current status
- Early development: primary milestone is a stable JS build system/backend for static pages and JS output.
- Syntax and semantics are still shifting; some constructs (e.g., closures, interfaces) are not final or fully implemented in the pipeline.

## What we're building
- A high-level language with templates as first-class citizens and ownership treated as an optimisation (GC is the fallback).
- Near-term target is a stable JS backend/build system for static pages and JS output; Wasm remains the long-term primary target.
- Build systems can use the compiler up through HIR (and borrow checking) and then apply their own codegen for any backend, including potential Rust-interpreter-backed builds.
- A modular compiler exposed as a library, plus a build system and CLI that assemble single-file and multi-file projects into runnable bundles.

## System shape
- **CLI entrypoint**: `src/main.rs` delegates to `src/cli.rs` for commands (`new`, `build`, `run`, `release`, `dev`, `tests`). CLI only orchestrates; it never owns compiler logic.
- **Build orchestration**: `src/build.rs` selects a `BuildTarget` (HTML project, embedded, JIT), reads configs, loads modules, and hands work to a `ProjectBuilder`.
- **Build system targets**: `src/build_system/` holds target-specific builders and tooling:
  - `core_build.rs` shared helpers; `embedded_project.rs` for host embedding; `jit_wasm.rs` and `repl.rs` for execution without emitting files.
  - `html_project/` bootstraps HTML projects and dev assets, including syntax highlighting glue.
- **Compiler pipeline** (`src/compiler/`):
  - Parsers: `parsers/tokenizer/*` → `parsers/parse_file_headers.rs` → `parsers/ast*.rs` (builds typed AST, does constant folding in `optimizers/constant_folding.rs`).
  - Dependency sort: `module_dependencies.rs` orders headers before AST/HIR creation.
  - HIR: `hir/*` linearises control flow and marks possible drops.
  - Borrow checker: `borrow_checker/*` validates ownership for optimisation (not correctness-critical).
  - LIR + backends: `lir/*` lowers HIR; `codegen/js` and `codegen/wasm` emit targets; `html5_codegen/*` covers HTML glue. Non-bundled build systems should still reuse the pipeline through HIR/borrow checking before their own codegen.
  - Support: `compiler_messages/*`, `datatypes.rs`, `string_interning.rs`, `interned_path.rs`, `host_functions/registry.rs`.
- **Tests and fixtures**:
  - Rust-side integration tests in `src/compiler_tests/*` (JS backend, name hygiene) plus the `tests` CLI command that runs `tests/cases` via `test_runner.rs`.
  - Language fixtures live in `tests/cases/{success,failure}` with expectations described in `tests/cases/README.md`.
- **Docs**: Key references live in `docs/Beanstalk Compiler Design Overview.md`, `docs/Beanstalk Memory Management.md`, `docs/Wasm Codegen Overview.md`, and the Codebase Style Guide.

## Flow of work
- CLI parses args → `build::build_project_files` determines config and target → `ProjectBuilder` loads source modules → compiler pipeline produces HIR (and optional LIR) → backend emits JS/Wasm/HTML (or custom target) → build system writes outputs to `dev/` or `release/`.
- Dev server (`src/dev_server.rs`) is a thin wrapper that reuses the build system for hot iterations.

## Guardrails for new code
- Preserve stage boundaries: parsing/AST stages should not assume ownership resolution; HIR inserts drop points; borrow checker and LIR own ownership decisions; codegen consumes HIR/LIR without reaching back up-stack.
- Treat the compiler as a library: CLI and build_system should call into `build.rs` and compiler modules rather than re-implementing logic.
- All backends (built-in or external) should reuse the pipeline through HIR and borrow checking so semantic rules stay consistent regardless of target or host.
- Keep host interfaces centralised in `compiler/host_functions/registry.rs`; avoid ad-hoc imports elsewhere.
- When adding a new feature, prefer a new module under the existing stage (e.g., a parser helper under `parsers/` or a HIR transform under `hir/`) over cross-cutting utilities.
- Maintain single responsibility per file; if a file starts mixing concerns, split by stage or data shape (tokens vs AST vs HIR vs LIR).

## Extension playbook
- **New syntax or typing rule**: update tokenizer → header parser → AST typing; add/adjust HIR lowering and borrow/LIR rules as needed; extend fixtures in `tests/cases` and, if backend-specific, add a Rust test in `src/compiler_tests`.
- **New build target or output shape**: add a `BuildTarget`, implement a `ProjectBuilder`, and keep shared logic in `build_system/core_build.rs` rather than in the CLI.
- **New host capability**: register it in `host_functions/registry.rs`, plumb it through HIR nodes, and cover it with a JIT test (`build ... --target Jit`) or a fixture.

## Testing expectations
- Fast checks: `cargo run -- tests` to execute fixture suites via the CLI; `cargo test` for Rust unit/integration tests.
- Use flags (`--ast`, `--hir`, `--lir`, etc.) when debugging stage outputs; keep them confined to debugging code paths so normal builds remain quiet.

## Related documents to consult
- Compiler stages and memory model: `docs/Beanstalk Compiler Design Overview.md`, `docs/Beanstalk Memory Management.md`.
- Backend specifics: `docs/Wasm Codegen Overview.md`.
- Conventions and naming: `docs/Beanstalk Compiler Codebase Style Guide.md`.
