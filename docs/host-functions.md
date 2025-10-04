# Host Functions in Beanstalk

Host functions provide a bridge between Beanstalk programs and the runtime environment, enabling access to I/O operations, system calls, and other platform-specific functionality.

## Available Host Functions

### I/O Functions

#### `print(message: String)`
Prints a message to standard output with a newline.

```beanstalk
print("Hello, World!")
print("Debug value: " + value)
```

**Module**: `beanstalk_io`  
**Import Name**: `print`  
**Parameters**: 
- `message`: String to print
**Returns**: None

## Usage in Development

### Basic Usage

Host functions can be called directly in Beanstalk code:

```beanstalk
-- Simple print
print("Starting program...")

-- Print variables
name = "Beanstalk"
print("Language: " + name)
```

### Debugging with Print Statements

The `print` function is particularly useful for debugging:

```beanstalk
-- Debug variable values
x = 42
print("x = " + x)

-- Debug program flow
print("Entering main logic...")
-- ... your code ...
print("Exiting main logic...")
```

## Compilation and Execution

### Build with Host Functions

Host functions are automatically included when compiling Beanstalk programs:

```bash
# Build a program using host functions
cargo run -- build my_program.bst

# Run with JIT (includes host function implementations)
cargo run -- run my_program.bst
```

### Verbose Logging

To see detailed information about host function processing, use verbose logging:

```bash
# Enable verbose codegen logging to see host function processing
cargo run --features "verbose_codegen_logging" -- build my_program.bst
```

This will show:
- MIR processing of host function calls
- WASM generation for host function imports and calls
- Runtime execution of host functions

### Testing

Host functions work in all test scenarios:

```bash
# Run all tests (includes programs with host functions)
cargo run -- tests

# Test a specific file with host functions
cargo run -- build tests/cases/my_test.bst
```

## Implementation Details

### Compilation Pipeline

Host functions are processed through the entire compilation pipeline:

1. **AST Stage**: Host function calls are recognized and validated against the built-in registry
2. **MIR Stage**: Host functions become import declarations and call statements
3. **WASM Stage**: Import section is generated with correct signatures and call instructions
4. **Runtime Stage**: Backend-specific implementations are provided through Wasmer's import system

### Import Modules

Host functions are organized into WASM import modules:

- `beanstalk_io`: I/O operations (print, file operations)
- `beanstalk_env`: Environment variables
- `beanstalk_sys`: System operations (exit, etc.)

### Memory Interface

Host functions that work with strings automatically handle WASM memory:
- String arguments are passed as pointer + length pairs
- The runtime reads strings from WASM linear memory
- Memory access is bounds-checked for safety

## Troubleshooting

### Common Issues

1. **Host function not found**: Ensure the function name is spelled correctly and is in the built-in registry
2. **Type mismatch**: Check that argument types match the function signature
3. **Runtime errors**: Enable verbose logging to see detailed execution information

### Debugging Tips

1. Use verbose logging to trace host function processing:
   ```bash
   cargo run --features "verbose_codegen_logging" -- build my_program.bst
   ```

2. Check that host functions are being imported:
   - Look for "MIR: Added host function" messages
   - Verify WASM import generation

3. Verify runtime execution:
   - Look for "RUNTIME: Host function" messages during JIT execution

## Future Extensions

The host function system is designed to be extensible. Future additions may include:

- File I/O functions (`read_file`, `write_file`)
- Network operations
- System information queries
- Custom host function registration

## Examples

### Simple Program with Print

```beanstalk
-- hello.bst
print("Hello from Beanstalk!")
```

### Debug Program Flow

```beanstalk
-- debug_example.bst
print("Program starting...")

x = 10
y = 20
result = x + y

print("Calculation: " + x + " + " + y + " = " + result)
print("Program finished.")
```

### Testing Host Functions

```beanstalk
-- test_print.bst
-- Test host function call
print("Testing print function")

-- Test with variables
message = "Variable message"
print(message)

-- Test with expressions
print("Expression result: " + (5 + 3))
```