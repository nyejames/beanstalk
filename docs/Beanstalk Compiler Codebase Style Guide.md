# Beanstalk Compiler Development Guide
This guide covers the structure, goals and best practices for Beanstalk's compiler development.

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
- Comments should use correct grammar without dropping definitive articles or connectives (`BAD:// Build AST` vs `GOOD:// Builds the AST`)

### Import Guidelines
- **Avoid inline imports**: If a function/type is used more than once in a file, import it at the top
- **Use clear, consistent names**: Avoid aliasing types or imports as much as possible

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

**Types in a file (structs / enums) should be ordered from the highest abstraction to the lowest**
```rust
// Define the most abstract types earlier in the file
// And define the types they depend on later
pub struct HirModule { ... }
pub struct HirBlock { ... }
pub struct HirNode { ...}
pub struct HirExpression { ...}
```

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
- Labelling parts of the control flow (branches) to clarify what each branch is doing
- TODOs for unimplemented features
- Comments referencing an unusual or unclear bit of code and why it is written the way it is. Particularly when something has been changed to fix a subtle bug.

### Returning Errors
The error system is built around three core types:
- **`CompilerError`**: The unified error type with owned data and structured metadata
- **`ErrorLocation`**: Owned location information without string interning dependencies
- **`ErrorMetaDataKey`**: Structured metadata keys for intelligent error analysis

CompilerError Best practices:
- **Be Specific**: Include exact tokens, types or names in errors.
- **Be Helpful**: Suggest corrections when possible, especially for borrow checker errors. Provide actionable messages with context
- **User errors**: Use `return_syntax_error!`, `return_rule_error!`, or `return_type_error!`
- **Compiler bugs**: Use `return_compiler_error!` (prefix added automatically)
- Always include source location (ErrorLocation) for user errors
- Use consistent error handling patterns across stages, use provided macros and methods inside `src/compiler/compiler_messages/compiler_errors.rs` to do this cleanly and consistently. 
- Each macro can support multiple variations, but sometimes using CompilerError methods directly will be more concise and clear for advanced error handling
- Return a CompilerMessages Err result when a mix of warnings and/or multiple errors can be created at once. Use a single CompilerError when only one error without warnings could be returned.
- Add warnings to the output when it is appropriate to warn rather than error. See `src/compiler/compiler_messages/compiler_warnings.rs`.

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
cargo run --features "show_ast,show_hir" -- run tests/cases/success/multi_file_module

# Show borrow checker and codegen output on single test file, ignore warnings
RUSTFLAGS="-A warnings" cargo run --features "detailed_timers,show_hir,show_borrow_checker" -- run tests/cases/test.bst
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