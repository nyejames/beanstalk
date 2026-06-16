# Beanstalk Compiler Development Guide
This guide defines the required standards for new and refactored compiler code.

Priorities:
1. Readability
2. Modularity
3. Correctness and diagnostics

Validate all changes with `just validate` which automatically runs:
- `cargo clippy`
- `cargo test`
- `cargo run tests`

For frontend boundary cleanup work, perform an explicit manual stage-boundary review alongside
`just validate`. Check that AST/HIR diagnostic and type-representation paths still respect their
owners, that user-facing diagnostics stay on `CompilerDiagnostic`, that infrastructure failures
stay on `CompilerError`, and that obsolete APIs or duplicated boundary paths have not been left
behind.

## No user-input panics
- Active frontend stages must reject unsupported syntax and malformed input with structured diagnostics, not `panic!`, `todo!`, or user-data-driven `.unwrap()`
- Panic paths are only for proven internal invariants that indicate a compiler bug

## Diagnostics
- Use `CompilerDiagnostic` for normal user-facing source, config, import, syntax, type, rule, borrow, and deferred-feature diagnostics.
- Use `CompilerError` only for internal compiler, filesystem, backend, dev-server, and tooling infrastructure failures.
- Diagnostic payloads and reason enums should carry structured facts, not pre-rendered prose.
- Preserve `SourceLocation` and source labels wherever the compiler can point to useful source context.
- Type diagnostics should carry semantic `TypeId`s and render them through `DiagnosticRenderContext`, not cloned `DataType` values or formatted type names.
- Use `DiagnosticBag` for stage-local accumulation and `CompilerMessages` only at build/render boundaries.
- Integration failure fixtures should prefer stable `diagnostic_codes` over rendered text when asserting expected user diagnostics.

## Naming
- Use descriptive, full names. Avoid abbreviations except simple iterators such as `i` and `j`
- Functions should be self-describing through clear names
- Compiler passes should use explicit names such as `build_ast`, `generate_hir`, and `emit_wasm`

Avoid vague names when they cross more than a few lines or function boundaries:
- prefer `function_signature` over `sig`
- prefer `expression` over `expr` outside tiny scopes
- prefer `data_type` or the concrete type name over `ty`
- prefer `scope_context` or `module_environment` over `ctx` or `env` when several contexts/environments exist

## Format! and printing
- Use the saying library macro `say!()` for std out when creating user facing messages that may need color styling in the future

Use variables directly in format! strings (in position) whenever possible.

## Imports
- Avoid inline imports. Only acceptable if they are one level deep and help with local reasoning:

```rust
// ACCEPTABLE: builtin_type_ids is imported, but kept inline because its terse and clear
let typeId = builtin_type_ids::BOOL

// BAD: large inline import, even if this is only used once, it's too noisy to use inline
let typeId = crate::compiler_frontend::datatypes::ids::builtin_type_ids::BOOL

// BAD: inline imports with long paths, should be imported at the top even if only used once
fn lookup_constant(
    &self,
) -> Option<(
    crate::compiler_frontend::external_packages::ExternalConstantId,
    &crate::compiler_frontend::external_packages::ExternalConstantDef,
)> {
    ...
}
```

- Avoid aliasing unless it clearly improves readability

## Code Style and Organisation
- Maintain clear separation between compilation stages
- Each module should have one clear responsibility. Do not mix concerns
- Split files by task category. Aim for files under ~2000 lines where practical
- Tests never live with production code, they should always have their own separate files. This includes utility functions that are only used by tests.
- `mod.rs` should be the module entry point and structural map: it exposes the public surface, shows the flow of the module, and points to the files that contain the real implementation. Keep it focused on orchestration, re-exports, and documentation rather than core implementation
- A reader should be able to open `mod.rs` first and quickly understand what the module does, which stages or responsibilities it contains, and where to find the important functions and types
- Use comments and doc comments in `mod.rs` to explain the module’s structure, data flow, and why the pieces are arranged that way
- Prefer context structs for shared state instead of passing many state values between functions
- Separate unrelated statements and all functions with an extra newline
- Avoid `.unwrap()` unless it is blatantly safe and tied to an internal invariant
- Prefer `.to_owned()` over `.clone()` when copying owned string-like data
- Use `.clone()` when a general copy is genuinely required and clearer

## Readable Rust style
Compiler code should optimize for fast human review. Dense code is harder to scan and easier for agents to extend badly.

Prefer code that reads as a sequence of named steps over dense expression chains, clever iterator nesting, or large inline matches. The best code in this compiler should make the data flow obvious before the reader understands every detail.

Good compiler code has:
- clear stage ownership
- named intermediate values
- short functions with one job
- explicit control flow for multi-step logic
- narrow helper functions
- context/input structs instead of long parameter lists
- enums for meaningful states instead of loose booleans
- comments that act as reading landmarks
- vertical spacing between logical blocks (add blank lines between logical steps)

Avoid code that compresses too much logic into one expression. Brevity is less important than clarity.

## Vertical spacing
Use vertical spacing to show structure.

Required:
- one blank line between functions
- one blank line between unrelated statement groups
- one blank line between major control-flow blocks inside long functions
- one blank line between match arms unless the arms are tiny and visually identical
- one blank line before a comment that introduces a new logical step

Prefer this:

```rust
let source_file = header.source_file.clone();
let visibility = import_environment.visibility_for(&source_file)?;

// Resolve type aliases before constants because constant folding may depend on alias-expanded types.
self.resolve_type_aliases(&headers, visibility, string_table)?;

self.resolve_constant_headers(&headers, visibility, string_table)?;
```

Avoid this:

```rust
let source_file = header.source_file.clone();
let visibility = import_environment.visibility_for(&source_file)?;
self.resolve_type_aliases(&headers, visibility, string_table)?;
self.resolve_constant_headers(&headers, visibility, string_table)?;
```

## Comments as reading landmarks
Comments are encouraged when they make code faster to understand. Always:
- Add doc comments at the top of new files
- Add concise WHAT/WHY comments for complex functions, structs, methods, and important control-flow joins

Be comment-positive, but not noisy. Explain behavior and rationale, don't restate syntax. Add comments when they help a reader understand code locally without reading large parts of the codebase first. They should explain:
- what role this code plays in the larger subsystem
- which later stage, caller, or output consumes the result
- why the control flow is shaped this way
- invariants
- failure conditions
- subtle bug fixes
- non-obvious data flow
- important joins between phases
- unusual or project specific code, subtle bug fixes, invariants, dataflow direction, and failure conditions
- code used widely across the repository

Use comments to mark:
- stage boundaries
- non-obvious ordering requirements
- invariants
- why a fallback exists
- why a branch intentionally does nothing
- why an error is reported at this stage instead of a later stage
- control-flow joins in large functions
- the reason a helper exists

Good comments are grammatical, readable and explain intent:

```rust
// Register same-file declarations before imports so aliases cannot shadow local names.
for declaration in same_file_declarations {
    visible_names.register_same_file_declaration(declaration)?;
}

// Prelude names are registered as reserved names first, then inserted only if not shadowed.
for symbol in external_package_registry.prelude_symbols() {
    visible_names.reserve_prelude_symbol(symbol)?;
}
```

## Refactor moves
When moving code across compiler stages, do not copy the old module shape unchanged.
Use the move to correct ownership, names, APIs, comments, and data flow.

Moved code must:
- remove obsolete wrappers and compatibility paths
- replace long parameter lists with context/input structs
- split mixed-responsibility modules
- update file-level docs to match the new owner
- delete the old owner once the new path is wired

Do not preserve a bad API shape just because it already exists.

## API Breakage
- Beanstalk is pre-release. Backward compatibility is not a priority
- When APIs change, thread the new shape through the compiler and remove the old one
- Do not add compatibility wrappers, forwarding shims, parallel structs, or defaulted legacy entry points just to preserve an older interface
- Prefer one current API shape, not transitional layers

## Type Ordering
Order types in a file from higher-level abstractions to lower-level supporting types.

```rust
pub struct HirModule { ... }
pub struct HirBlock { ... }
pub struct HirNode { ... }
pub struct HirExpression { ... }
```

## Iterators vs Loops
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

## Function Size
Functions should usually stay under ~200 lines but longer functions are acceptable when they still represent one coherent operation, such as a compiler transformation, state machine, or tightly coupled sequential process.

Split functions when they mix unrelated responsibilities, are hard to test, or no longer match their name

## Match readability
Large matches should be grouped by meaning.

Use blank lines and short comments to make each group obvious. If a match grows too large, extract branch handling into named helpers.

Prefer:

```rust
match resolution {
    // Constants create dependency edges.
    ConstantReference::SourceConstant { path, source_file } => {
        add_constant_dependency(header, path, source_file)?;
    }

    // Type names are valid to resolve but do not create value dependency edges.
    ConstantReference::SourceTypeAlias { .. } => {}

    // These are structurally invalid in constant initializers.
    ConstantReference::SourceNonConstant { path }
    | ConstantReference::ExternalNonConstant { .. } => {
        errors.push(non_constant_reference_error(path, location.clone()));
    }

    // Unknown names may produce better diagnostics during AST expression parsing.
    ConstantReference::Unknown => {}
}
```

Avoid long ungrouped matches where every branch looks equally important.

## Avoid clever Rust
Do not use advanced Rust features just to make code shorter.

Avoid:
- nested iterator chains with side effects
- clever `Option` / `Result` combinator pipelines for validation logic
- large closures with mutation
- broad generic helpers that hide compiler-stage ownership
- macro abstractions for ordinary compiler flow
- boolean-heavy APIs where an enum would explain the state
- tuple-heavy returns where a named struct would explain the result

Prefer named types and straightforward flow:

```rust
pub(crate) struct ImportResolution {
    pub(crate) target: ResolvedImportTarget,
    pub(crate) local_name: StringId,
    pub(crate) location: SourceLocation,
}
```

instead of:

```rust
Result<(ResolvedImportTarget, StringId, SourceLocation), CompilerError>
```

A few more lines are acceptable if the result is easier to inspect and harder to misuse.

## Macros
- Keep macro usage minimal
- Use small declarative macros only where they clearly reduce repetition
- Avoid creating procedural macros

## Warnings and Lints
- Use `clippy`
- Use the default Rust formatter
- Keep unused variables and dead code to a minimum
- Use `#[allow(dead_code)]` only with clear justification. Dead code must have a comment with it stating this is a todo or used only in tests

## Section banners
Use section banners only in long functions, large files, or complex orchestration code where phase boundaries help a reader jump into the process. Use exactly this format:

```rust
// ------------------------
//  Resolve template slots
// ------------------------
```

Rules:
- Three lines.
- Top and bottom lines use `// ` followed by dashes.
- The title line starts with `//  ` and uses one extra leading space after the comment marker.
- Dash lines should exceed the title text by the same amount on each side.
- Use title case or clear imperative phase names consistently within the file.
- Do not use banners around small helpers.
- Do not use banners to hide a file that should be split.

Good uses:
- parser phases
- compiler lowering phases
- template folding stages
- import resolution stages
- diagnostic rendering stages
- long build orchestration functions

## Returning Errors
The diagnostic system has two paths:
- **`CompilerDiagnostic`** — current user-facing diagnostic type. Use typed payloads
  (`DiagnosticPayload`) and stable diagnostic codes. This is the correct path for all
  source-language diagnostics (syntax, type, rule, import, borrow, config).
- **`CompilerError`** — internal/tooling failures only (compiler bugs, HIR lowering
  failures, filesystem errors, backend failures). Do not route user-facing diagnostics through
  `CompilerError`.

### Rules for new code
- **New user-facing diagnostics must use `CompilerDiagnostic`** with typed constructors
  in `compiler_diagnostic.rs`. Do not add new `CompilerError` for syntax, type, rule,
  import, or borrow diagnostics.
- Use `return_compiler_error!` **only** for internal compiler bugs or broken invariants.
- Use `DiagnosticBag` for stage-local accumulation of multiple diagnostics.
- Convert to `CompilerMessages` only at clear build/render boundaries.
- When a local `Result` error boundary carries `CompilerDiagnostic` or `CompilerError` and
  Clippy reports `result_large_err`, box the payload inside the local boundary enum or use a
  stage-local boxed diagnostic result alias. Keep `DiagnosticBag` and `CompilerMessages` owning
  plain `CompilerDiagnostic` values at accumulation and render/build boundaries.
- Prefer `Option<CompilerDiagnostic>` over `Result<(), CompilerDiagnostic>` for predicate-only
  validation helpers that only need to report one diagnostic or continue.
- Prefer explicit loops over iterator closures when the closure would infer
  `Result<_, CompilerDiagnostic>`.
- Include a `SourceLocation` for every user-facing diagnostic.
- Keep paths interned until render time. Do not duplicate them as owned `PathBuf`s.
- New type diagnostics must carry semantic `TypeId`s and context enums, not rendered type strings
  or cloned `DataType` payloads. Render type names through the diagnostic render context.
- Be specific: include exact tokens, types, or names.
- Be helpful: suggest corrections when practical.
- Use stable diagnostic codes in integration tests where practical (see `diagnostic_codes`
  in `expect.toml`).

### Examples

```rust
// Typed diagnostic constructor
return Err(CompilerDiagnostic::invalid_assignment_target(
    InvalidAssignmentTargetReason::ImmutableVariable,
    variable_name,
    location,
));

// Internal bug path (permanent)
return_compiler_error!(
    "Unsupported AST node type: {:?}",
    node_type; {
        CompilationStage => "AST Processing",
        PrimarySuggestion => "This is a compiler bug - please report it"
    }
);
```

## Testing Workflow
The primary goal is end-to-end language correctness. Prefer real usage patterns and full language snippets over narrow isolated tests.

### Test ownership and pruning
Use one clear owner for each behavior:

| Behavior | Owner |
|---|---|
| Pure data/invariant behavior | Focused unit test near that subsystem. |
| Compiler stage invariant | Stage-local unit test with the invariant named in the test or a short comment. |
| Stage-boundary smoke path | Minimal pipeline/build test that proves orchestration, not feature behavior. |
| User-facing language behavior | Integration case under `tests/cases`. |
| Backend artifact behavior | Backend-specific integration assertions or goldens, unless the unit test protects backend-internal planning policy. |

Retain unit tests only when they protect an internal invariant, impossible state, side-table fact,
or backend policy that integration output cannot inspect directly. Split broad cross-stage test
support into focused modules when import usage shows real stage boundaries, and localize one-caller
fixtures instead of creating shared helpers. Prefer `tempfile::tempdir()` for tests that create
directories or files; use unmanaged temp-path helpers only when the caller intentionally needs a
unique path and owns cleanup. HIR tests should assert relationships such as branch joins, merge
locals, and jump arguments instead of exact block indexes unless exact layout is the invariant.

### Unit Testing
- Do not keep tests in the same files as production code
- Module-specific tests should live in that module’s test directory, for example `src/compiler_frontend/hir/tests/`
- End-to-end or multi-module tests should live in `src/compiler_tests/`
- Once a subsystem is stable, prune outdated unit tests to avoid long-term test bloat
- Rewriting tests is preferable to carrying obsolete ones forward
- Prefer integration tests whenever possible

### Integration Testing
Integration tests are the main regression check for new features and refactors.

- `cargo run -- tests` runs the integration test runner in `src/compiler_tests`
- Tests should use real Beanstalk snippets
- Canonical cases should be self-contained directories representing one scenario each
- Multi-file fixtures should remain inside one case folder so helpers are not treated as standalone tests
- Failure cases for ordinary source/config/import/type/rule/borrow diagnostics should assert stable `diagnostic_codes`; add rendered message fragments only when the rendered text itself is the behavior under test. Infrastructure-boundary tests may still assert infrastructure `ErrorType` classifications
- Always add strong output assertions when possible
- Use strict goldens only when exact emitted text is contractual. Prefer rendered-output assertions for behavior-first cases
- Avoid using console host functions such as `io.line(...)` unless they are explicitly what is being tested. Prefer top-level templates to simulate output since this shows up in emitted artifacts

For new type-system syntax, add both:
- positive end-to-end usage tests
- negative diagnostics for value/type namespace misuse, import visibility, duplicate declarations, and cross-file resolution

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

## Checklist
Before finishing a non-trivial change, check the touched code against this list:
- The module has one clear owner responsibility
- The file-level doc says what the module owns and what it must not own
- The main function reads as a sequence of named steps
- Complex blocks have short intent comments
- No fully inline imports
- Match arms are grouped by meaning
- There is whitespace between unrelated blocks
- Function arguments are not noisy or repetitive
- Booleans are not standing in for named states
- Error construction is not duplicated across branches
- The code avoids cleverness that makes review slower
- There are no stale comments from the old design
- There are no compatibility wrappers preserving obsolete paths
- Tests cover behavior, not implementation accidents
- Clippy lints are not being suppressed
