# Beanstalk Compiler Development Guide

This guide defines the required standards for new and refactored compiler code.

Priorities:
1. Readability
2. Modularity
3. Correctness and diagnostics

Before finishing changes, always run:
- `cargo clippy`
- `cargo test`
- `cargo run tests`

Or run `just validate`, which covers all of the above plus `cargo fmt --check`, the docs build, and the speed test.
You must have `just` installed to run this.

## Best Practices

### No user-input panics
- Active frontend stages must reject unsupported syntax and malformed input with structured diagnostics, not `panic!`, `todo!`, or user-data-driven `.unwrap()`
- Panic paths are only for proven internal invariants that indicate a compiler bug

### Naming
- Use descriptive, full names. Avoid abbreviations except simple iterators such as `i` and `j`
- Functions should be self-describing through clear names
- Compiler passes should use explicit names such as `build_ast`, `generate_hir`, and `emit_wasm`

### Format! and printing
- Use the saying library macro `say!()` for std out when creating user facing messages that may need color styling in the future

Use variables directly in format! strings whenever possible:
```rust
    format!("Rust interpreter lowering could not resolve assignment target local {local_id:?}");
```

### Imports
- Avoid inline imports. If a type or function is used more than once in a file, import it at the top
- Avoid aliasing unless it clearly improves readability

### Code Style and Organisation
- Maintain clear separation between compilation stages
- Each module should have one clear responsibility. Do not mix concerns
- Split files by task category. Aim for files under ~2000 lines where practical
- `mod.rs` should be the module entry point and structural map: it exposes the public surface, shows the flow of the module, and points to the files that contain the real implementation. Keep it focused on orchestration, re-exports, and documentation rather than core implementation
- A reader should be able to open `mod.rs` first and quickly understand what the module does, which stages or responsibilities it contains, and where to find the important functions and types
- Use comments and doc comments in `mod.rs` to explain the module’s structure, data flow, and why the pieces are arranged that way
- Prefer context structs for shared state instead of passing many state values between functions
- Separate unrelated statements and all functions with an extra newline
- Avoid `.unwrap()` unless it is blatantly safe and tied to an internal invariant
- Prefer `.to_owned()` over `.clone()` when copying owned string-like data
- Use `.clone()` when a general copy is genuinely required and clearer

### API Breakage
- Beanstalk is prealpha. Backward compatibility is not a priority
- When APIs change, thread the new shape through the compiler and remove the old one
- Do not add compatibility wrappers, forwarding shims, parallel structs, or defaulted legacy entry points just to preserve an older interface
- Prefer one current API shape, not transitional layers

### Type Ordering
Order types in a file from higher-level abstractions to lower-level supporting types.

```rust
pub struct HirModule { ... }
pub struct HirBlock { ... }
pub struct HirNode { ... }
pub struct HirExpression { ... }
```

### Iterators vs Loops
- Use iterators for simple transformations
- Use explicit loops for complex multi-stage logic where control flow is clearer

```rust
let mut processed_nodes = Vec::new();
for node in ast_nodes {
    if let Some(ir_node) = convert_to_ir(&node)? {
        if ir_node.is_optimizable() {
            processed_nodes.push(optimize_node(ir_node));
        }
    }
}
```

### Function Size
- Simple functions should usually stay under ~50 lines
- Complex functions should usually stay under ~100 lines
- Longer functions are acceptable when they still represent one coherent operation, such as a compiler transformation, state machine, or tightly coupled sequential process
- Split functions when they mix unrelated responsibilities, are hard to test, or no longer match their name

### Macros
- Keep macro usage minimal
- Use small declarative macros only where they clearly reduce repetition
- Avoid procedural macros

### Comments
- Add doc comments at the top of new files
- Add concise WHAT/WHY comments for complex functions, structs, methods, and important control-flow joins
- Comments should explain behavior and rationale, not restate syntax
- Use comments to clarify unusual code, subtle bug fixes, invariants, dataflow direction, and failure conditions
- Use grammatical, readable comments

```rust
// GOOD:
// Builds the AST for the current module before name resolution.

// BAD:
// Build AST
```

### Warnings and Lints
- Use `clippy`
- Use the default Rust formatter
- Keep unused variables and dead code to a minimum
- Use `#[allow(dead_code)]` only with clear justification. Dead code must have a comment with it stating this is a todo or used only in tests

## Returning Errors

The compiler error system is based on:
- `CompilerError` for structured owned errors
- `SourceLocation` for source spans and file-level diagnostic locations
- `ErrorMetaDataKey` for structured metadata
- `CompilerMessages` for aggregated warnings/errors plus the shared `StringTable` needed to render interned paths at boundaries

Rules:
- Be specific. Include exact tokens, types, or names
- Be helpful. Suggest corrections when practical
- Use stage-appropriate `return_*_error!` macros for user-facing errors
- Use `return_compiler_error!` only for internal compiler bugs or broken invariants
- Always include a `SourceLocation` for user errors
- If a diagnostic does not come from a parsed token span, create a file-level `SourceLocation` by interning the path into the current build's shared `StringTable`
- Keep diagnostic paths interned until render time. Do not duplicate them as owned `PathBuf`s on warnings/errors
- Use the shared error helpers in `src/compiler_frontend/compiler_messages/compiler_errors.rs` for consistency
- Return `CompilerMessages` when producing multiple warnings and/or errors together, and preserve the associated `StringTable` when crossing build/rendering boundaries
- Return a single `CompilerError` when only one error without warnings is needed
- Emit warnings when warning-level behavior is more appropriate than failure. See `src/compiler_frontend/compiler_messages/compiler_warnings.rs`

Error categories include:
`Syntax`, `Type`, `Rule`, `File`, `Config`, `Compiler`, `DevServer`, `BorrowChecker`, `HirTransformation`, `LirTransformation`, and `WasmGeneration`

Examples:
```rust
return_syntax_error!(
    "Expected ';' after statement",
    location, {
        CompilationStage => "Parsing",
        PrimarySuggestion => "Add a semicolon at the end of the statement"
    }
);

return_compiler_error!(
    "Unsupported AST node type: {:?}",
    node_type; {
        CompilationStage => "AST Processing",
        PrimarySuggestion => "This is a compiler bug - please report it"
    }
);
```

## Development Commands and Feature Flags

```bash
cargo run --features "show_ast,show_hir,detailed_timers" -- build test.bst
cargo run -- tests
cargo run -- build tests/cases/basic_print_statement/input/main.bst
```

Useful debug flags:
- `show_tokens`
- `show_headers`
- `show_ast`
- `show_hir`
- `detailed_timers`

See `Cargo.toml` for the full feature set.

## Testing Workflow

The primary goal is end-to-end language correctness. Prefer real usage patterns and full language snippets over narrow isolated tests.

### Unit Testing
- Do not keep tests in the same files as production code
- Module-specific tests should live in that module’s test directory, for example `src/compiler_frontend/hir/tests/`
- End-to-end or multi-module tests should live in `src/compiler_tests/`
- Once a subsystem is stable, prune outdated unit tests to avoid long-term test bloat
- Rewriting tests is preferable to carrying obsolete ones forward
- Prefer integration tests whenever possible

### Integration Testing
Integration tests are the main regression check for new features and refactors.

- `cargo run tests` runs the integration test runner in `src/compiler_tests`
- Tests should use real Beanstalk snippets
- Canonical cases should be self-contained directories representing one scenario each
- Multi-file fixtures should remain inside one case folder so helpers are not treated as standalone tests
- Failure cases should assert the intended `ErrorType` and, where practical, message fragments proving the correct failure reason
- Always add strong output assertions when possible
- Use strict goldens only when exact emitted text is contractual. Prefer rendered-output assertions for behavior-first cases
- Avoid using host functions like io() unless they are explicitly what is being tested. Prefer top-level templates to simulate output since this shows up in emitted artifacts

**Test Case Structure** (`tests/cases/`):
```text
tests/cases/
├── manifest.toml
└── case_name/
    ├── input/
    │   ├── main.bst
    │   └── helper.bst
    ├── expect.toml
    └── golden/
        ├── html/
        └── html_wasm/
```

### Backend Matrix Expectations
Use one `expect.toml` per case with backend-specific assertion blocks.

```toml
entry = "."

[backends.html]
mode = "success"
warnings = "forbid"

[backends.html_wasm]
mode = "success"
warnings = "forbid"

[[backends.html_wasm.artifact_assertions]]
path = "page.wasm"
kind = "wasm"
validate_wasm = true
must_export = ["memory", "bst_str_ptr", "bst_str_len", "bst_release"]
```

Store matrix-mode goldens in `golden/<backend>/...` so one input fixture can assert different backend outputs.