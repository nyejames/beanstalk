# Rust Interpreter Implementation Plan

The interpreter should stay a **backend-local execution path over validated HIR**, not a second frontend. The compiler pipeline already treats backend lowering as the stage after HIR and borrow validation, and build systems consume HIR rather than parsing or semantically compiling themselves . Keep that boundary strict.

Main rule: implement the interpreter as **GC-semantic first**. Ownership and drop behavior are optimization layers later; the memory design explicitly says GC is the semantic baseline and ownership is optional runtime state, not required for correctness.

## Current State

The current module shape is good:

```text
src/backends/rust_interpreter/
├── backend.rs
├── ctfe/
├── debug.rs
├── error.rs
├── exec_ir/
├── heap/
├── lowering/
├── request.rs
├── result.rs
├── runtime/
├── tests/
└── value/
```

The backend entry point already exposes lowering plus optional execution, with request/result/debug seams in place . `request.rs` already anticipates both normal execution and CTFE policy modes . Exec IR has been split into `types.rs` and `instructions.rs`, which is the right direction .

Do not add more high-level concepts until the foundation issues below are fixed.

---

# Stage 0 — Foundation Cleanup Before More Features

## Goal

Remove structural debt introduced during arithmetic lowering before internal calls are added.

This stage should be one small PR.

## Required fixes

### 0.1 Remove or replace `scratch_local_id`

Current layout still has both:

* `scratch_local_id`
* dynamic temp locals

But temp locals start at the same id as scratch locals. `scratch_local_id` is set to `ordered_hir_local_ids.len()`, and `allocate_temp_local` also starts from `ordered_hir_local_ids.len() + temp_index` . `functions.rs` then pushes the scratch local and later appends temp locals, so ids can collide .

**Preferred fix:** remove persistent scratch locals completely.

Replace with:

```rust
pub(crate) fn allocate_temporary_local(
    &mut self,
    storage_type: ExecStorageType,
) -> ExecLocalId
```

Use temporary locals for:

* expression statement discard
* return literal materialization
* branch condition materialization
* intermediate arithmetic results

Delete:

* `scratch_local_id`
* `ExecLocalRole::InternalScratch`, unless another real internal local role needs it soon
* scratch-specific tests

### 0.2 Restore `Load` vs `Copy` semantics

`ExecValue` is currently only:

```rust
Literal(...)
Local(...)
```

That collapses HIR `Load(Local)` and `Copy(Local)` into the same lowering result . Then assignment materializes `ExecValue::Local` through `CopyLocal` . This will be wrong for heap handles and function calls.

Replace with:

```rust
pub(crate) enum LoweredExpressionValue {
    Literal(ExecConstValue),
    LocalRead(ExecLocalId),
    LocalCopy(ExecLocalId),
}
```

Then materialization should choose:

```rust
LocalRead(source) => ExecInstruction::ReadLocal { target, source }
LocalCopy(source) => ExecInstruction::CopyLocal { target, source }
```

This matters because Beanstalk uses shared access by default and explicit copies only when requested .

### 0.3 Move materialization helpers out of `expressions.rs`

Create:

```text
src/backends/rust_interpreter/lowering/materialize.rs
```

Own these helpers there:

```rust
lower_expression_to_temporary(...)
materialize_expression_value(...)
storage_type_for_const(...)
```

This keeps `expressions.rs` focused on recursive expression shape lowering.

### 0.4 Make integer arithmetic panic-safe

Runtime still uses raw integer operations such as `l + r`, `l - r`, `l * r`, and `-v` . Use checked operations:

```rust
checked_add
checked_sub
checked_mul
checked_div
checked_rem
checked_neg
```

Return structured runtime errors on overflow. The style guide forbids user-input-driven panics and requires structured diagnostics/errors instead .

### 0.5 Split arithmetic runtime dispatch

`runtime/engine.rs` is already getting heavy. Move operation execution into:

```text
src/backends/rust_interpreter/runtime/operators.rs
```

Keep `engine.rs` responsible for frame/block dispatch. Put value arithmetic/comparison logic in `operators.rs`.

## Tests

Add/adjust:

```text
src/backends/rust_interpreter/tests/local_materialization_tests.rs
src/backends/rust_interpreter/tests/runtime_operator_tests.rs
```

Required cases:

* `Load(Local)` assignment emits `ReadLocal`
* `Copy(Local)` assignment emits `CopyLocal`
* first temporary local id does not collide with user locals
* no scratch local exists in lowered function locals
* integer add/sub/mul/neg overflow returns runtime error
* `i64::MIN // -1` returns runtime error
* `i64::MIN % -1` returns runtime error if using checked remainder

## Done when

* no scratch-local id collision exists
* `Load` and `Copy` are distinct in Exec IR
* arithmetic cannot panic in debug builds
* `cargo clippy`, `cargo test`, and `cargo run tests` pass, as required by the codebase guide 

---

# Stage 1 — Internal User Function Calls

## Goal

Support direct calls between Beanstalk functions in the same lowered module.

Do this before strings/templates. Calls are the runtime backbone.

## Scope

Support:

* call expressions returning one value
* call statements returning unit
* positional arguments only at first
* same-module user functions
* normal GC-style shared value passing
* explicit copy arguments when HIR says copy

Do not support yet:

* host calls
* receiver methods
* multi-return
* error returns
* mutable/exclusive ownership behavior
* recursion limits beyond a simple max-call-depth policy

## Exec IR changes

Add to `ExecInstruction`:

```rust
Call {
    function: ExecFunctionId,
    arguments: Vec<ExecLocalId>,
    destination: Option<ExecLocalId>,
}
```

This is enough for first call support.

Do **not** encode named arguments here. AST/HIR should already have resolved call structure. Exec IR should be execution-oriented.

## Lowering changes

Create:

```text
src/backends/rust_interpreter/lowering/calls.rs
```

Responsibilities:

* resolve `HirExpressionKind::Call` or current equivalent HIR call shape
* map HIR function id to `ExecFunctionId`
* lower each argument expression left-to-right
* materialize each argument into a temporary local
* allocate return destination when call appears in expression position
* emit `ExecInstruction::Call`

If the call result is unit, destination is `None`.

## Runtime changes

Current `execute_function` rejects functions with parameters . Replace this with frame setup that accepts argument values.

Add:

```rust
fn execute_function_with_arguments(
    &mut self,
    function_id: ExecFunctionId,
    arguments: Vec<Value>,
) -> Result<Value, InterpreterBackendError>
```

Runtime call instruction algorithm:

1. Read argument locals from current frame.
2. Clone/share values according to current GC baseline.
3. Push new frame for callee.
4. Populate parameter slots.
5. Run callee until return.
6. Pop callee frame.
7. Write return value into destination, if present.

Keep the execution model simple for now: nested `execute_function_with_arguments` is acceptable. A manual frame return state machine can come later when needed.

## Runtime safety

Add execution policy limit:

```rust
max_call_depth: usize
```

Put it in `InterpreterExecutionPolicy` only if needed. Otherwise a fixed internal constant is fine for now.

Return an error like:

```text
Rust interpreter runtime exceeded maximum call depth
```

No panic.

## Tests

Add:

```text
src/backends/rust_interpreter/tests/function_call_tests.rs
```

Required cases:

* start calls `add_one(41)` and returns `42`
* function call used inside arithmetic expression
* call statement to unit function
* nested function call
* function with two parameters
* wrong argument count in malformed Exec IR returns runtime error
* execution of specific function entry remains unsupported or is implemented fully

## Done when

* functions with parameters execute
* calls compose with arithmetic
* all argument locals are materialized explicitly
* no special frontend logic is added

---

# Stage 2 — Runtime Value Semantics and Heap Discipline

## Goal

Make value movement safe enough for heap-backed values before string/template execution.

Current `Value` is small and explicit: primitives plus `Handle` . Keep that shape. Do not add ownership flags yet.

## Scope

Support GC-style heap handles:

* shared handle reads
* handle assignment by reference
* explicit copy failure for heap values unless a real clone path exists
* debug rendering for handles
* heap object lookup errors

## Changes

### 2.1 Define value transfer operations

Create:

```text
src/backends/rust_interpreter/runtime/values.rs
```

Add helpers:

```rust
read_shared_value(...)
copy_value(...)
write_value(...)
```

`ReadLocal` should copy the enum value, meaning a handle remains a shared handle.

`CopyLocal` should:

* copy primitives
* reject heap handles until clone semantics exist

This preserves the language rule that copies are explicit .

### 2.2 Add handle validation

When reading heap objects, invalid handles must return structured runtime errors.

### 2.3 Add debug rendering seam

Create:

```text
src/backends/rust_interpreter/runtime/display.rs
```

or add to `value/` if it is pure value formatting.

Support:

```rust
fn format_runtime_value(value: &Value, heap: &Heap) -> String
```

Useful for tests and later CTFE diagnostics.

## Tests

Add:

```text
src/backends/rust_interpreter/tests/value_semantics_tests.rs
src/backends/rust_interpreter/tests/heap_tests.rs
```

Required cases:

* shared read of handle succeeds
* explicit copy of handle errors
* invalid heap handle errors
* returned string handle can be rendered through debug helper

## Done when

* calls can safely pass heap handles by shared reference
* explicit copy remains strict
* heap errors are structured

---

# Stage 3 — String Runtime and Minimal Template Execution

## Goal

Support the language’s core value: runtime string/template construction.

Beanstalk’s language design puts templates at the center of UI generation and string formatting . But keep this stage minimal: support runtime string production, not full HTML builder fragment assembly.

## Scope

Support:

* string literals as heap string handles
* simple template expression lowering if HIR exposes it
* append primitive/string values into a builder
* finalize builder into string handle
* return string from `start` or user functions

Do not support yet:

* top-level page fragment assembly
* markdown/html/css formatter behavior
* slots/wrappers at runtime
* collection of runtime fragments
* full builder integration

The compiler design says AST owns template composition/folding and HIR only lowers finalized runtime templates that remain . Respect that. The interpreter should execute HIR’s remaining runtime template representation; it should not reconstruct AST template logic.

## Exec IR changes

Add:

```rust
ExecInstruction::NewStringBuilder {
    destination: ExecLocalId,
}

ExecInstruction::AppendStringPart {
    builder: ExecLocalId,
    value: ExecLocalId,
}

ExecInstruction::FinishStringBuilder {
    builder: ExecLocalId,
    destination: ExecLocalId,
}
```

Add heap object:

```rust
HeapObject::StringBuilder(StringBuilderObject)
```

Or keep builder as a runtime-only object if that is cleaner.

## Lowering changes

Create:

```text
src/backends/rust_interpreter/lowering/templates.rs
```

Lower HIR runtime template expressions into:

1. `NewStringBuilder`
2. append lowered parts
3. `FinishStringBuilder`

Use existing frontend string coercion expectations. Do not invent new coercion rules in the interpreter.

## Runtime behavior

Append rules:

* `String` handle appends string content
* string literal handle appends string content
* `Int`, `Float`, `Bool`, `Char` append formatted value
* unsupported heap objects return structured errors

## Tests

Add:

```text
src/backends/rust_interpreter/tests/string_runtime_tests.rs
src/backends/rust_interpreter/tests/template_runtime_tests.rs
```

Required cases:

* return string literal
* return `[:hello]`
* return template with int capture
* return nested template if HIR already exposes it cleanly
* append unsupported value returns error

## Done when

* string-returning functions execute
* template runtime lowering works for simple HIR cases
* no HTML builder-specific behavior exists in the interpreter

---

# Stage 4 — Branches, Loops, and Match Execution

## Goal

Complete ordinary control flow before expanding data types.

The runtime already supports `Jump` and `BranchBool` terminators . HIR has explicit blocks and terminators, with match represented as a terminator according to the compiler design .

## Scope

Support:

* loops already lowered to blocks/jumps
* computed branch conditions
* `break` / `continue` already mapped to jumps
* match terminator lowering for literal patterns
* match guards
* choice variant match later if choice values are not ready yet

Do not support:

* full destructuring
* capture/tagged patterns
* ownership/drop-on-control-flow semantics

## Changes

### 4.1 Clean control-flow tests

Add explicit runtime tests for:

* while-style loop lowered to blocks
* range loop if HIR lowers it through existing statements/blocks
* break
* continue
* nested branch

### 4.2 Implement `HirTerminator::Match`

Current match lowering is pending .

Add Exec IR terminator:

```rust
Match {
    scrutinee: ExecLocalId,
    arms: Vec<ExecMatchArm>,
    fallback: Option<ExecBlockId>,
}
```

Start with literal comparisons only:

```rust
ExecMatchPattern::Int(i64)
ExecMatchPattern::Float(f64)
ExecMatchPattern::Bool(bool)
ExecMatchPattern::Char(char)
ExecMatchPattern::String(ExecConstId)
```

For guards, lower guard expression into a condition local inside a prelude block only if HIR already permits it. Compiler docs say match guard lowering must remain pure for terminator conditions and guards that lower with prelude statements are rejected . Do not bypass that.

## Tests

Add:

```text
src/backends/rust_interpreter/tests/control_flow_tests.rs
src/backends/rust_interpreter/tests/match_tests.rs
```

Required cases:

* if true / false branches
* loop increments local until condition false
* match int literal
* match bool literal
* match with else
* match no selected arm errors only if malformed HIR says no fallback

## Done when

* ordinary structured control flow executes
* literal match executes
* unsupported match patterns produce pending-lowering or structured errors

---

# Stage 5 — Host Calls and Builtin Runtime Boundary

## Goal

Support the minimum host/builtin interface needed for useful execution and CTFE restriction checking.

HIR preserves builtins such as `io` as explicit call nodes, and no abstraction layer exists between HIR and host calls . The interpreter needs its own host-call registry, not ad-hoc matching inside the engine.

## Scope

Support:

* `io(...)` in normal headless mode
* CTFE mode rejecting side-effecting host calls
* structured host-call result values
* captured output in `InterpreterExecutionResult`

Do not support:

* filesystem
* time/random
* network
* platform host APIs

## Files

Create:

```text
src/backends/rust_interpreter/runtime/host.rs
src/backends/rust_interpreter/lowering/host_calls.rs
```

Add to `InterpreterExecutionResult`:

```rust
pub(crate) captured_output: Vec<String>
```

or a small `RuntimeOutput` struct.

## Exec IR

Add:

```rust
HostCall {
    host_function: ExecHostFunctionId,
    arguments: Vec<ExecLocalId>,
    destination: Option<ExecLocalId>,
}
```

Start with:

```rust
enum ExecHostFunctionId {
    Io,
}
```

## Runtime

Normal mode:

* `io(value)` appends rendered value to captured output
* returns unit

CTFE mode:

* side-effecting calls return an error:

```text
Rust interpreter CTFE cannot execute side-effecting host function io
```

## Tests

Add:

```text
src/backends/rust_interpreter/tests/host_call_tests.rs
src/backends/rust_interpreter/tests/ctfe_policy_tests.rs
```

Required cases:

* normal `io("hello")` captures output
* `io(42)` captures `42`
* CTFE rejects `io`
* unknown host call errors clearly

## Done when

* normal execution can observe output
* CTFE policy has its first real enforcement point

---

# Stage 6 — Structs and Choices

## Goal

Add enough aggregate values to execute ordinary user-defined code.

## Scope

Support:

* struct construction
* field read
* field write only when HIR has already validated mutability
* choice construction
* choice variant equality/discriminant tests
* choice match

Do not support:

* methods as dynamic dispatch
* trait/runtime dispatch
* structural typing
* per-field borrow/ownership analysis

Runtime structs should remain nominal, matching the language guide .

## Heap/value model

Add:

```rust
HeapObject::Struct(StructObject)
HeapObject::Choice(ChoiceObject)
```

With ids:

```rust
ExecStructId
ExecChoiceId
ExecChoiceVariantId
ExecFieldId
```

Do not use strings for field lookup at runtime once lowered. Resolve to ids during lowering.

## Exec IR

Add instructions:

```rust
ConstructStruct
ReadField
WriteField
ConstructChoice
ReadChoiceDiscriminant
```

Keep field/variant metadata in `ExecModule`.

## Lowering

Create:

```text
src/backends/rust_interpreter/lowering/aggregates.rs
src/backends/rust_interpreter/exec_ir/metadata.rs
```

## Tests

Add:

```text
src/backends/rust_interpreter/tests/struct_tests.rs
src/backends/rust_interpreter/tests/choice_tests.rs
```

Required cases:

* construct struct and return field
* assign field, then return field
* construct choice with no payload
* construct choice with payload
* match choice variant
* wrong field id in malformed Exec IR errors

## Done when

* ordinary structs and choices execute
* match can handle choice discriminants
* all runtime aggregate lookups are id-based

---

# Stage 7 — Collections and Ranges

## Goal

Support the data structures needed for loops, simple programs, and returned fragment lists.

The compiler design says entry `start()` eventually returns `Vec<String>` for runtime page fragments, and builders merge compile-time fragments around that list . The interpreter will need collection support before it can be used for that path.

## Scope

Support:

* collection literal
* `length`
* `get`
* `set`
* `push`
* `remove`
* range iteration if not already lowered away
* structured bounds errors

Do not support:

* generic runtime reflection
* heterogeneous collection values unless HIR already permits them
* ownership-optimized element movement

## Runtime

Add:

```rust
HeapObject::Collection(CollectionObject)
```

Collection object:

```rust
pub(crate) struct CollectionObject {
    pub element_storage_type: ExecStorageType,
    pub values: Vec<Value>,
}
```

## Tests

Add:

```text
src/backends/rust_interpreter/tests/collection_tests.rs
src/backends/rust_interpreter/tests/range_loop_tests.rs
```

Required cases:

* collection literal returns length
* get valid index
* get invalid index returns structured error
* set valid index
* push/remove
* loop over collection if HIR exposes this as blocks plus builtin ops
* returned collection of strings

## Done when

* collection operations match documented runtime contracts
* invalid receiver/index errors are structured
* runtime fragment list representation becomes possible

---

# Stage 8 — CTFE Integration Surface

## Goal

Expose the interpreter as a controlled frontend service for compile-time evaluation.

Do **not** wire it deeply into AST until runtime behavior is stable. First expose a narrow API.

## API

In `ctfe/mod.rs`, define:

```rust
pub(crate) struct CtfeRequest {
    pub function: FunctionId,
    pub arguments: Vec<CtfeValue>,
}

pub(crate) struct CtfeResult {
    pub value: CtfeValue,
}
```

Keep `CtfeValue` separate from runtime `Value` at the boundary. Internally convert as needed.

## Allowed CTFE subset

Start with:

* primitives
* strings
* pure user functions
* arithmetic
* comparisons
* structs/choices if implemented
* no `io`
* no host side effects
* no unsupported heap mutation unless proven deterministic

## Integration rule

The frontend may request CTFE only after HIR exists for the target module/function. The interpreter must not parse source, inspect AST internals, or redo type checking.

## Tests

Add:

```text
src/backends/rust_interpreter/tests/ctfe_tests.rs
```

Required cases:

* pure const helper evaluates
* pure string helper evaluates
* CTFE rejects `io`
* CTFE rejects unsupported host call
* CTFE error reports function id/name clearly

## Done when

* CTFE has a narrow request/result API
* normal runtime and CTFE policy share the same engine
* CTFE restrictions are centralized

---

# Stage 9 — Integration Tests and Backend Wiring

## Goal

Move from unit-only confidence to language-level regression coverage.

The style guide says integration tests are the main regression check and should prefer real Beanstalk snippets over narrow isolated tests .

## Work

Add compiler integration cases only after a feature is usable through the normal frontend.

Suggested cases:

```text
tests/cases/rust_interpreter_arithmetic/
tests/cases/rust_interpreter_function_calls/
tests/cases/rust_interpreter_strings/
tests/cases/rust_interpreter_structs/
tests/cases/rust_interpreter_collections/
```

But avoid making the interpreter a public backend target too early. It can stay test-only until execution semantics are stable.

## Acceptance

* each stage has direct unit tests
* major user-visible behavior has integration tests
* no test fixture relies on unstable debug text unless debug output is the thing being tested

---

# Implementation Order Summary

## Immediate next PR

1. Remove scratch locals or reserve ids correctly.
2. Replace `ExecValue::Local` with distinct read/copy lowered values.
3. Move materialization helpers into `lowering/materialize.rs`.
4. Move runtime operator execution into `runtime/operators.rs`.
5. Add checked integer arithmetic.
6. Add focused tests for these cleanup fixes.

## Next feature PR

7. Add internal user function calls.
8. Add call frame argument setup and return destination writes.
9. Add call tests.

## Then

10. Stabilize heap/value movement.
11. Add string/template runtime.
12. Add match/control-flow completion.
13. Add host call boundary.
14. Add structs/choices.
15. Add collections.
16. Add CTFE request API.

---

# Guardrails for Agents

Every stage should follow these constraints:

* Keep changes inside `src/backends/rust_interpreter/` unless a tiny neutral HIR utility is genuinely needed.
* Do not add compatibility wrappers. This is pre-alpha, and the style guide explicitly says to thread new API shapes through directly .
* Prefer ids over names in Exec IR.
* Keep runtime behavior GC-first.
* Do not implement ownership flags, deterministic drops, or borrow re-analysis inside the interpreter yet.
* Keep `mod.rs` files as structure maps, not implementation buckets; the style guide explicitly calls this out .
* Split files when a file starts mixing responsibilities.
* No user-input panics.
* Before finishing each stage, run:

```bash
cargo clippy
cargo test
cargo run tests
```