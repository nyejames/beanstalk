# Beanstalk Compiler Development Guide
This guide covers the structure, goals and best practices for Beanstalk's compiler infrastructure.

## Compiler Goals
- Fast compilation speeds for development builds (as few passes as possible)
- Wasm focused backend design
- Compiler exposed as a library for ease of developing external tools like more build systems/LSPs etc...
- Compiler repo is bundled with a CLI and a complete build system with extensive tooling

Beanstalk makes deliberate tradeoffs for compilation speed:
- **Early constant folding** at AST stage eliminates optimisation passes
- **External optimisation** delegates complex transforms to WASM tooling, which will be used for release builds only

## Best Practices

### Variables and Functions
- Use descriptive, full names—avoid abbreviations except for simple iterators (`i`, `j`)
- Functions should be self-documenting through clear naming
- Compiler-specific prefixes: `ast_`, `hir_`, `lir_`, `wasm_` for clarity
- Compiler passes: descriptive names (`build_ast`, `generate_hir`, `emit_wasm`)

```rust
  // Good patterns for this codebase
  let ast_node = parse_expression(&tokens);
  let ir_instruction = build_ir_from_ast(&ast_node);
  let wasm_bytes = generate_wasm_from_ir(&ir_instruction);
```

### Import Guidelines
- **Avoid inline imports**: If a function/type is used more than once in a file, import it at the top
- **Use clear, consistent names**: Avoid aliasing types or imports as much as possible

```rust
// Bad: Inline imports for repeated usage
fn process_variable(name: &str) {
    let place = crate::compiler::hir::utilities::lookup_variable_or_error(
        context, name, location, string_table, "processing"
    )?;
    // ... more inline crate::compiler::hir::utilities:: calls
}
```

### Code Style and Organisation
**Compiler Development**:
- Maintain clear separation between compilation stages. Each compilation stage is independently exposed as a library in `src/lib.rs`.
- Follow the same style and patterns as the frontend of the codebase (`src/compiler/parsers`).
- Never use .unwrap() unless blatantly safe, prefer match to handle results.
- Prefer `.to_owned()` over `.clone()` for string/data copying (signals potential future refactoring)
- Use `.clone()` only when you are sure a copy is unavoidable with this pattern
- Split up and organise code into files that each deal with a category of tasks. Files should aim to be ~< 2000 lines each when possible.
- Each module has ONE clear responsibility—don't mix concerns
- Prefer using a context struct for state—don't pass individual state pieces between functions

#### Iterator vs Loop Preference
- Simple operations: Use iterators
- Complex multi-stage operations: Use explicit loops for clarity

```rust
// Prefer explicit loops for complex compiler logic
let mut processed_nodes = Vec::new();
for node in ast_nodes {
    if let Some(ir_node) = convert_to_ir(&node)? {
        if ir_node.is_optimizable() {
            processed_nodes.push(optimize_node(ir_node));
        }
    }
}
```

#### Function Size Guidelines
- **Simple functions**: ~50 lines max for straightforward operations
- **Complex functions**: ~100 lines max when handling complex tasks
- **Acceptable complexity**: Functions can be longer when they:
  - Handle intricate compiler transformations (AST → HIR, LIR → WASM)
  - Manage complex state machines or pattern matching
  - Coordinate multiple related operations that benefit from being together
  - Would be harder to understand if split into smaller pieces

**When to Split Functions**
- The function has multiple unrelated responsibilities
- The logic can be reused in other contexts
- The function is challenging to test as a whole
- The function name doesn't accurately describe what it does

**When to Keep Functions Together**
- Operations are tightly coupled and sequential
- Splitting would require passing many parameters
- The function represents a single conceptual operation
- Splitting would make the code harder to follow

### Macros
- Minimal macro usage: Only small declarative macros for repetition
- Avoid procedural macros entirely

### Other Style Considerations
- Use `#[allow(dead_code)]` sparingly with justification
- Try to reduce unused variable warnings as much as possible 
- Use clippy to check code
- Use the default Rust formatter on new code

### Comments:
Avoid over commenting code. Stick to concise and brief descriptions. 

Good places to add comments: 
- Short summaries before important / complex functions 
- Labelling parts of the control flow (branches) to make it clearer what each branch is doing
- TODOs for unimplemented features
- Comments referencing an unusual or unclear bit of code and why it is written the way it is. Particularly when something has been changed to fix a subtle bug.

### Returning Errors
The error system is built around three core types:
- **`CompileError`**: The unified error type with owned data and structured metadata
- **`ErrorLocation`**: Owned location information without string interning dependencies
- **`ErrorMetaDataKey`**: Structured metadata keys for intelligent error analysis

CompileError Best practices:
- **Be Specific**: Include exact tokens, types, or names in errors.
- **Be Helpful**: Suggest corrections when possible, especially for borrow checker errors. Provide actionable messages with context
- **User errors**: Use `return_syntax_error!`, `return_rule_error!`, or `return_type_error!`
- **Compiler bugs**: Use `return_compiler_error!` (prefix added automatically)
- Always include source location (ErrorLocation) for user errors
- Use consistent error handling patterns across stages, use provided macros and methods inside `src/compiler/compiler_errors.rs` to do this cleanly and consistently. 
- Each macro can support multiple variations, but sometimes using CompileError methods directly will be more concise and clear for advanced error handling
- Return a CompilerMessages Err result when a mix of warnings and/or multiple errors can be created at once. Use a single CompileError when only one error without warnings could be returned.
- Add warnings to the output when it is appropriate to warn rather than error. See `src/compiler/compiler_warnings.rs`.

Every error as an associated type which informs the error output formatter how to display it and what data to expect and display.
``` rust
pub enum ErrorType {
    Syntax,
    Type,
    Rule,
    File,
    Config,
    Compiler,
    DevServer,
    BorrowChecker,
    HirTransformation,
    LirTransformation,
    WasmGeneration,
}
```

```rust
// Good: User made a syntax error with metadata
return_syntax_error!(
    "Expected ';' after statement",
    location, {
        CompilationStage => "Parsing",
        PrimarySuggestion => "Add a semicolon at the end of the statement"
    }
);

// Bad: Using compiler error for user mistakes
return_compiler_error!("User provided invalid variable name"); // Should be rule_error!

// Instead, Use for: Internal compiler bugs and unimplemented features
// Examples: Unsupported AST nodes, internal state corruption
// User-facing: No - indicates the compiler developer needs to fix
// Note: Automatically prefixed with "COMPILER BUG" in output
// No location required: These are internal errors
return_compiler_error!(
    "Unsupported AST node type: {:?}",
    node_type; {
        CompilationStage => "AST Processing",
        PrimarySuggestion => "This is a compiler bug - please report it"
    }
);

// Bad: Using rule error for unimplemented features  
return_rule_error!(location, "Match expressions not supported"); // Should be hir_transformation_error!
```

## Compilation Pipeline Stages
### Overview
The build system will determine which files are associated with a single Wasm module.
Those files are then all tokenized, parsed into headers and have their dependencies sorted. 
After this, everything is combined into a single AST that should be able to check all types and see all declarations in the module.

- **Single WASM Output**: All files compile to one WASM module with proper function exports
**Entry Point Semantics**:
- Each file has an **implicit start function** containing its top-level code
- Every file in the module's implicit start function becomes `HeaderKind::StartFunction`
- Imported files' implicit start functions are callable but don't execute automatically
- Only one entry point is allowed per module, 
this is the `HeaderKind::Main` and is implicit start function of the entry file

The Beanstalk compiler processes modules through these stages:
1. **Tokenization** - Convert source text to tokens
2. **Header Parsing** - Extract headers and identify the entry point. Separates function definitions, structs, constants from top-level code
3. **Dependency Sorting** - Order headers by dependencies
4. **AST Construction** - Build an abstract syntax tree
5. **HIR Generation** - Create a high-level IR designed for borrow checking and lifetime analysis
6. **Borrow Checking** - Verify memory safety
7. **LIR Generation** - Create an IR close to Wasm that can be lowered directly
8. **Codegen** - Produce final Wasm bytecode

**Key Pipeline Principles**:
- **Import Resolution**: Processes `#import "path"` statements at the header stage so dependencies can be sorted after
- **Early optimization**: Constant folding and type checking at AST stage
- **Module-aware compilation**: Header parsing enables multi-file modules with proper entry point designation
- **No optimization passes in IR**: Complex optimizations left to external WASM tools for release builds only

### Stage 1: Tokenization (`src/compiler/parsers/tokenizer.rs`)
**Purpose**: Convert raw source code into structured tokens with location information.

**Key Features**:
- Precise source location tracking for error reporting
- Recognition of Beanstalk-specific syntax (`:`, `;`, `~`, `#import`)
- Context switching for delimiter handling (`[]` vs `""`)

**Development Notes**:
This stage of the compiler is stable and currently can represent almost all the tokens Beanstalk will need to represent.

---

### Stage 2: Header Parsing (`src/compiler/parsers/parse_file_headers.rs`)
**Purpose**: Extract function definitions, structs, constants, imports and identify entry points before AST construction.

**Key Features**:
- **Header Extraction**: Separates declarations from top-level code
- **Implicit Start Function**: Top level code that does not fit into the other header catagories is placed into a  `HeaderKind::StartFunction` header that becomes a public "start" function.
- **Entry Point Detection**: Identifies entry file and converts its start function to `HeaderKind::Main`
- **Import Resolution**: Processes `#import "path/function_name"` directives
- **Dependency Analysis**: Builds import graph and detects circular dependencies

**Development Notes**:
Use `show_headers` feature flag to inspect parsed headers.

```rust
pub enum HeaderKind {
    Function(FunctionSignature, Vec<Token>),
    Template(Vec<Token>), // Top level templates are used for HTML page generation
    Struct(Vec<Arg>),
    Choice(Vec<Arg>),
    Constant(Arg),

    // The top-level scope of regular files.
    // Any other logic in the top level scope implicitly becomes a "start" function.
    // This only runs when explicitly called from an import.
    // Each .bst file can see and use these like normal functions.
    // Start functions have no arguments or return values
    // and are not visible to the host from the final wasm module.
    StartFunction(Vec<Token>),

    // This is the main function that the host environment can use to run the final Wasm module.
    // The start function of the entry file.
    // It has the same rules as other start functions,
    // but it is exposed to the host from the final Wasm module.
    Main(Vec<Token>),
}
```

---

### Stage 3: Dependency Sorting (`src/lib.rs::sort_headers`)
**Purpose**: Order headers topologically to ensure proper compilation sequence so the AST for the whole module can be created in one pass. This enables the AST to perform full type checking.

**Key Features**:
- Topological sort of import dependencies
- Circular dependency detection
- Entry point validation (single entry per module)

---

### Stage 4: AST Construction (`src/compiler/parsers/ast.rs`)
**Purpose**: Transform headers into Abstract Syntax Tree with compile-time optimizations.

**Key Features**:
- **Header Integration**: Convert headers to AST nodes
- **Entry Point Handling**: StartFunction and Main headers are parsed into normal functions and given a reserved name. Only the main function is exposed to the host.
- **Constant Folding**: Immediate evaluation of compile-time expressions
- **Type Checking**: Early type resolution and validation

**Compile-Time Folding**: The AST stage performs aggressive constant folding in `src/compiler/optimizers/constant_folding.rs`:
- Pure literal expressions (e.g., `2 + 3`) are evaluated immediately
- Results in `ExpressionKind::Int(5)` rather than runtime operations
- Expressions are converted to **Reverse Polish Notation (RPN)** for evaluation

**Runtime Expressions**: When expressions cannot be folded at compile time:
- Variables, function calls or complex operations become `ExpressionKind::Runtime(Vec<AstNode>)`
- The `Vec<AstNode>` contains the expression in **RPN order** ready for stack-based evaluation
- Example: `x + 2 * y` becomes `[x, 2, y, *, +]` in the Runtime vector

**Type System Integration**: 
- Type checking occurs during AST construction
- `DataType` information is attached to all expressions
- Type mismatches are caught early in the pipeline

**Development Notes**:
- Use `show_ast` feature flag to inspect generated AST

---

## Stage 5: HIR Generation (`src/compiler/hir/`)
HIR (High-Level IR) is Beanstalk’s semantic IR.  
It is the first fully typed, canonical representation of a program, designed for **borrow checking, last-use analysis, and structured lowering**.

HIR is still “Beanstalk-shaped”: it preserves high-level constructs, references, structured control flow and the semantics needed for ownership analysis.

### Purpose
- Represent Beanstalk programs in a form that is straightforward to analyze but not yet bound to Wasm.
- Provide a stable shape for borrow checking, move analysis and template resolution.
- Normalise syntax sugar into canonical nodes while preserving structure.

### Key Features
- **Structured Control Flow**: `if`, `loop`, `match`, and template bodies remain structured for analysis.
- **Place-based Memory Model**: Locals, temporaries, and fields represented as “places” for borrowing analysis.
- **Ownership & Move Tracking**: HIR nodes explicitly mark moves, borrows, and drops derived from last-use analysis.
- **Template Handling**: Compile-time templates are folded; runtime templates are converted into HIR function bodies.
- **Desugared Semantics**: Error/option propagation, assignment sugar, and borrowing rules lowered into a uniform form.
- **Not Tied to Wasm**: No stack machine semantics, no Wasm types, no address arithmetic.

### Debugging HIR
- Use `show_hir` to inspect the desugared program.
- Verify move and borrow annotations.
- Inspect template lowering, constant folding, and structured control flow.

---

## Stage 6: Borrow Checking (`src/compiler/borrow_checker/`)
The borrow checker operates *exclusively on HIR*.  
Its job is to validate ownership correctness before the program is lowered toward Wasm.

### Purpose
- Enforce Beanstalk’s reference rules.
- Ensure all moves and borrows are consistent with lifetimes derived from HIR.
- Reject programs with illegal aliasing or dangling references.

### Key Features
- **Region & Lifetime Inference**: Implicit lifetimes derived from structure and usage.
- **Loan Tracking**: Mutable/immutable borrow conflicts detected via place-based loans.
- **Move Validation**: Ensure moved-from values are not used afterwards.
- **Drop Insertion Points**: Determine where cleanup must occur (for later LIR lowering).
- **Origin Checking for Returns**: Validate reference-returning functions use the declared origin parameter correctly.

### Development Notes
- All borrow errors must reference original source spans, not HIR internals.
- Complex features (pattern binds, runtime templates) require additional region facts.
- HIR must remain easy for the borrow checker to walk — keep constructs structured, not flattened.

---

## Stage 7: LIR Generation (`src/compiler/lir/`)
LIR (Low-Level IR) is the *Wasm-shaped* representation of the program.  
It is a close, structural match to Wasm’s execution model, with stack effects and control blocks fully explicit.

LIR contains no remaining high-level constructs from Beanstalk: everything has been lowered into concrete Wasm-compatible operations.

### Purpose
- Transform HIR into an instruction-level IR that can be directly emitted as Wasm.
- Make control flow, locals and memory operations explicit.
- Insert drops, compute field offsets and rewrite multi-value returns.

### Key Features
- **Wasm-Friendly Control Flow**: Blocks, loops, and branches match Wasm’s structured CFG.
- **Concrete Memory Access**: All field and array accesses lowered to explicit offsets and load/store instructions.
- **Explicit Locals**: All temporaries materialized as Wasm locals or stack values.
- **Drop Semantics**: Ownership outcomes from HIR lowering translated to explicit drop/free operations.
- **Stack Discipline**: Expressions sequenced according to Wasm’s operand stack rules.
- **Final Type Model**: All values lowered to Wasm types (`i32`, `i64`, `f32`, `f64`, plus reference types if enabled).

### Debugging LIR
- Use `show_lir` to inspect Wasm-shaped blocks.
- Verify stack height balancing.
- Confirm struct layouts and offsets.
- Inspect lowered drop instructions and ownership decisions.

---

## Stage 8: Codegen (`src/compiler/codegen/`)
Transforms LIR directly into Wasm bytecode.

### Purpose
- Encode LIR instructions into valid WebAssembly.
- Produce linear memory layout, data segments, function tables and exports.

### Key Features
- **Direct Encoding**: LIR nodes correspond 1:1 (or close) to Wasm bytecode s

## Beanstalk Memory Model and Borrow Semantics
Beanstalk uses a borrow checker to enable performant automatic memory management and memory safety, while also eliminating entire classes of bugs.

### Rules
### 1. Shared References (Default)
- Borrowing is the Default
- Multiple shared references to the same data are allowed
- Shared references are read-only access
- Created by default assignment: `x = y`
- Last-use analysis determines when they can be "killed"
- **No explicit `&` or `&mut` operators** - these don't exist in Beanstalk
- All variable usage creates immutable references by default

### 2. Mutable Access (`~` syntax)
- Mutability is always explicit
- Use `~` to indicate mutable access (reference or ownership)
- Only one mutable access allowed at a time
- Mutable access is exclusive (no other references allowed)
- Created by mutable assignment: `x ~= y`
- Compiler determines if this becomes ownership transfer or mutable reference based on static analysis

### 3. Ownership Transfer (Moves)
- Moves transfer ownership completely
- Original variable becomes unusable until reassigned
- Compiler statically determines if this becomes ownership transfer (move) or mutable reference
- Created by move assignment: `x ~= ~y`
- Cannot move while any borrows exist

### 4. Copies are Explicit
- No implicit copying for any types unless they are part of an expression creating a new value out of multiple references, or when used inside a template head
- All types require explicit copy semantics when copying is needed
- Most operations use borrowing instead of copying

### 5. Unified ABI for Moves and Mutable References
Beanstalk does not generate separate function bodies for “owned” vs “borrowed” arguments. 
Function signatures make no distinction between a mutable reference or a move (owned value). Instead, all function calls use a single ABI:

- Arguments that live in linear memory are passed as tagged pointers.
- The lowest alignment-safe bit of the pointer is used as an ownership flag (1 = owned, 0 = borrowed).
- The callee masks out the tag to recover the real pointer.
- If the ownership bit is set, the callee is responsible for dropping the value before returning.
- Borrow checker rules guarantee that the caller no longer uses owned arguments after the call.

This design keeps dispatch static, avoids monomorphization and prevents binary-size growth on Wasm while still allowing the compiler to freely choose between moves and mutable references based on last-use analysis.

### Assignment Operations
```beanstalk
data = [:hello]           -- data owns the immutable string

-- SHARED REFERENCES (default)
ref1 = data             -- ref1 references data (shared)
ref2 = data             -- ref2 also references data (shared) - OK
result = ref1           -- result references ref1 (which references data)

-- MUTABLE ACCESS (explicit with ~)
mut_ref ~= data         -- mut_ref gets mutable access to data
-- Compiler determines: reference or ownership based on data's future usage
-- ERROR: mut_ref conflicts with existing shared references (ref1, ref2)
```

### Correct Usage Patterns
```beanstalk
-- Pattern 1: Sequential usage (no conflicts)
data = "hello"
ref1 = data             -- shared reference
use(ref1)               -- use the reference
-- ref1's last use - compiler can "kill" it
mut_ref ~= data         -- now mutable access is OK

-- Pattern 2: Disjoint field access
person = Person { name: "Alice", age: 30 }
name_ref = person.name      -- reference just the name field
age_ref ~= person.age       -- mutable access to age field - OK (different fields)

-- Pattern 3: Compiler-determined ownership transfer
data = "hello"
ref1 = data
use(ref1)               -- last use of ref1
moved ~= data           -- Compiler determines: ownership transfer (data's last use)
-- OR: moved gets mutable reference (if data used later)
```

### Key Differences from Rust
| Aspect | Rust | Beanstalk |
|--------|------|-----------|
| Borrow syntax | `&x`, `&mut x` | `x` (shared), `x ~=` (mut) |
| Default semantics | Move | Borrow |
| Explicit operations | Borrow | Mutability/Move |
| Copy behavior | Implicit for Copy types | Always explicit |

This memory model provides memory safety while maintaining Beanstalk's goal of minimal, intuitive syntax.

## Development Commands and Feature Flags

### Basic Development Commands

```bash
# Compile and run a single file
cargo run -- run test.bst

# Compile and run with debugging output
cargo run --features "show_ast,show_hir,detailed_timers" -- run test.bst

# Run integration test suite
cargo run -- run tests

# Run specific test case
cargo run -- run tests/cases/success/basic_print.bst
```

### Feature Flags for Debugging
See the Cargo.toml for all feature flags.

**Compilation Pipeline Debugging**:
- `show_tokens` - Display tokenization output
- `show_headers` - Display parsed headers and dependencies
- `show_ast` - Display generated Abstract Syntax Tree
- `show_hir` - Display High level IR
- `show_wasm` - Display generated WASM bytecode

**Performance Analysis**:
- `detailed_timers` - Show timing for each compilation stage
- `memory_usage` - Track memory usage during compilation

**Error Debugging**:
- `verbose_errors` - Extended error information with stack traces
- `borrow_checker_debug` - Detailed borrow checking analysis

### Example Debug Session
```bash
# Debug header parsing issues
cargo run --features "show_headers,verbose_errors" -- run main.bst

# Debug entry point detection
cargo run --features "show_headers,show_ast" -- run main.bst

# Debug import resolution
cargo run --features "show_headers,show_ast,verbose_errors" -- run main.bst

# Full pipeline debugging
cargo run --features "show_headers,show_ast,show_hir,detailed_timers" -- run main.bst
```

## Testing Workflow
The primary goal is to get the language working end-to-end. Focus on real-world usage patterns and language features.

### Unit Testing (`src/compiler_tests`)
Unit testing should be used only to check new compiler features work as expected, but not used extensively.
The tests should always be stored inside src/compiler tests, and never inline with actual code.

Once a system is working as expected, old unit tests should be pruned to reduce graudal unit test bloat.
Rewriting unit tests is preferable to leaving them in the codebase, 
and using integration tests with actual language snippets should always be prefered.

### Integration Testing
Integration tests are the main way to check new features or refactors still work.
They run actual snippets of Beanstalk code.

Using `cargo run -- run tests` starts the test runner inside src/compiler_tests and automatically run all the integration tests.
This will provide a percentage pass rate for both expected and unexpected failures.

**Test Case Structure** (`tests/cases/`):
```
tests/cases/
├── success/
│   ├── basic_print.bst           # Simple print statement
│   ├── multi_file_module/        # Multi-file test
│   │   ├── main.bst
│   │   └── helper.bst
│   └── import_syntax.bst         # Import resolution test
└── failure/
    ├── circular_import/          # Circular dependency test
    │   ├── main.bst
    │   └── helper.bst
    └── missing_import.bst        # Missing import test
```

**Running Integration Tests**:
```bash
# Run all test cases
cargo run -- run tests

# Run with debugging
cargo run --features "show_headers,show_ast" -- run tests/cases/success/multi_file_module
```

When testing something new or experimenting with the language, 
there is a `tests/cases/test.bst` file that can be written over and used for quick testing in a project.
This is useful for highlighting a particular priority that is being worked on without yet adding the case to the list of tests.

cargo run --features "detailed_timers" -- run tests/cases/test.bst

Once an integration test case has been fixed, it should be added as a new test to the list of cases if there isn't already a similar integration test.

### Basic Beanstalk Debugging Examples
**Simple Value Inspection**:
```beanstalk
-- Debug variable values
count = 42
io(count)  -- Prints: 42

message = "Debug message"
io(message)  -- Prints: Debug message

result = true
io(result)  -- Prints: true
```

**Debugging with Labels**:
```beanstalk
-- Add context to debug output
count = 10
io([: Count: count])  -- Prints: Count: 10

status = "active"
io([: Status: status])  -- Prints: Status: active
```

**Debugging Function Execution**:
```beanstalk
calculate |x Int, y Int| -> Int:
    io([: Calculating: x + y])
    result = x + y
    io([: Result: result])
    return result
;

value = calculate(5, 3)
io([: Final value: value])
```

**Debugging Loops**:
```beanstalk
items = {1, 2, 3, 4, 5}

for item in items:
    io([: Processing item: item])
;

io("Loop complete")
```

### Still in the design phase
The implementation of the compiler, particularly the backend, is being rapidly iterated on beyond the core design choices of the language.

Here are some notes about the direction of future features:
- **Interfaces instead of traits** avoid complex trait resolution. Dynamic dispatch as the default for smaller binary sizes and faster compile times.
- **Simplified generics** reduce monomorphization overhead