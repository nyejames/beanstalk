---
inclusion: always
---

# Beanstalk Compiler Development Guide

## Naming Conventions

### Variables and Functions
- Use descriptive, full names - avoid abbreviations except for simple iterators (`i`, `j`)
- Functions should be self-documenting through clear naming
- Compiler-specific prefixes: `ast_`, `ir_`, `wasm_` for clarity

```rust
// Good patterns for this codebase
let ast_node = parse_expression(&tokens);
let ir_instruction = build_ir_from_ast(&ast_node);
let wasm_bytes = generate_wasm_from_ir(&ir_instruction);
```

### Types and Modules
- Types: `PascalCase` (`AstNode`, `IrInstruction`, `CompileError`)
- Functions/variables: `snake_case`
- Compiler passes: descriptive names (`build_ast`, `generate_ir`, `emit_wasm`)

## Architecture Patterns

### Compiler Pipeline Structure
Follow the established pipeline: **Source → AST → IR → WASM → Runtime**

- **AST nodes**: Define in `src/compiler/parsers/ast_nodes.rs`
- **IR nodes**: Define in `src/compiler/ir/ir_nodes.rs`
- **Codegen**: Separate modules for WASM (`src/compiler/codegen/`) and HTML5 (`src/compiler/html5_codegen/`)

### Node Implementation Requirements
All AST and IR nodes must include:
- Source location tracking (`SourceLocation` field)
- `Debug` and `Clone` trait implementations
- Consistent enum variants for node types

```rust
#[derive(Debug, Clone)]
pub struct AstNode {
    pub location: SourceLocation,
    pub node_type: AstNodeType,
    // ... other fields
}
```

## Error Handling Patterns

### Compiler Error Types and Macros
Use the appropriate error macro based on the error type:

- **`return_syntax_error!(location, "message", args...)`**: For syntax errors in user code (malformed syntax)
- **`return_rule_error!(location, "message", args...)`**: For rule violations in user code (semantic errors, undefined variables/functions)
- **`return_type_error!(location, "message", args...)`**: For type system violations in user code
- **`return_compiler_error!("message", args...)`**: For internal compiler bugs (should never be user's fault, "COMPILER BUG" prefix added automatically)
- **`return_file_error!(path, "message", args...)`**: For file system errors (missing files, permissions)

```rust
// Examples of proper error usage
pub fn parse_function(tokens: &[Token]) -> Result<AstNode, CompileError> {
    if tokens.is_empty() {
        return_syntax_error!(location, "Expected function definition, found end of input");
    }
    
    if !is_valid_function_name(&name) {
        return_rule_error!(location, "Function name '{}' is not valid", name);
    }
    
    if param_type == DataType::Inferred(_) {
        return_compiler_error!("Inferred type found at IR stage. Type inference should be complete.");
    }
}
```

### Error Guidelines
- **User errors**: Use `return_syntax_error!`, `return_rule_error!`, or `return_type_error!`
- **Compiler bugs**: Use `return_compiler_error!` (prefix added automatically)
- Always include source location for user errors
- Provide actionable error messages with context

## Memory Management Guidelines

### Smart Pointer Usage
- `Rc<RefCell<T>>`: Shared mutable state in compiler passes
- `Box<T>`: Recursive data structures (AST/IR trees)
- Prefer `.to_owned()` over `.clone()` for string/data copying
- Use `.clone()` only when copy is unavoidable (signals potential future refactoring)

### Iterator vs Loop Preference
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

## Testing Organization

### Test Structure
- Unit tests: Place in `src/compiler_tests/` directory (e.g., `ir_tests.rs`, `parser_tests.rs`)
- Integration tests: `tests/cases/` directory for Beanstalk language tests
- Each compiler pass should have comprehensive positive/negative test coverage

### Test File Patterns
- `src/compiler_tests/ir_tests.rs`: All IR-related unit tests
- `src/compiler_tests/parser_tests.rs`: Parser unit tests
- `tests/cases/test.bs`: Scratch file for development testing
- `tests/cases/*.bs`: Specific language feature tests

## Development Workflow

### Adding New Language Features
1. Define AST representation in `ast_nodes.rs`
2. Add parsing logic in appropriate parser module
3. Implement IR lowering in `build_ir.rs`
4. Add WASM codegen in `codegen/` modules
5. Add HTML5 codegen if web-relevant
6. Write comprehensive tests

### Code Quality Standards
- Functions: Single responsibility, ~50 lines max
- Minimal macro usage: Only small declarative macros for repetition
- Avoid procedural macros entirely
- Use `#[allow(dead_code)]` sparingly with justification

## Development Commands

### Testing Commands
```bash
# Test specific Beanstalk code (development)
cargo run --features "verbose_ast_logging,verbose_eval_logging,verbose_codegen_logging,detailed_timers" -- build tests/cases/test.bst

# Performance testing on all cases
cargo run --features "detailed_timers" -- build tests
```

### Feature Flags for Development
- `verbose_ast_logging`: AST construction details
- `verbose_eval_logging`: Expression evaluation tracing  
- `verbose_codegen_logging`: Code generation details
- `detailed_timers`: Performance profiling