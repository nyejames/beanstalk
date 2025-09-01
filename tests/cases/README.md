# Beanstalk Test Cases

This directory contains test cases for the Beanstalk compiler, organized into success and failure cases.

## Directory Structure

### `success/` - Tests that should compile successfully
- `basic_declarations.bs` - Basic variable declarations (mutable and immutable)
- `proper_mutations.bs` - Correct mutation syntax using `=` operator
- `basic_arithmetic.bs` - Arithmetic operations and expressions
- `correct_mutation_test.bs` - Proper mutable variable usage patterns
- `declarations_only.bs` - Variable declarations without mutations
- `basic_features.bs` - Comprehensive language feature tests
- `simple_arithmetic.bs` - Simple arithmetic with proper mutation syntax
- `basic_math.bs` - Mathematical operations and function exports
- `if_statements.bs` - Conditional statement tests

### `failure/` - Tests that should trigger compiler errors
- `invalid_reassignment_shadowing.bs` - Using `~=` for reassignment (should be error)
- `undefined_variable_mutation.bs` - Attempting to mutate undefined variables
- `immutable_mutation_attempt.bs` - Trying to mutate immutable variables
- `variable_redeclaration.bs` - Variable redeclaration/shadowing attempts
- `borrow_checker_use_after_move.bs` - Use after move violations
- `borrow_checker_multiple_mutable_borrows.bs` - Multiple mutable borrow conflicts
- `borrow_checker_mutable_immutable_conflict.bs` - Mutable/immutable borrow conflicts

### `test.bs` - Development scratch file
This file is used for development and debugging. It should contain valid Beanstalk code that compiles successfully.

## Language Rules Tested

### Variable Declaration and Mutation
- `~=` is ONLY for initial mutable variable declarations
- `=` is used for both immutable variable declarations AND mutable variable mutations
- Shadowing/redeclaration is not allowed
- Variables must be declared before use

### Borrow Checker Rules
- No use after move
- No multiple mutable borrows of the same data
- No simultaneous mutable and immutable borrows
- Proper lifetime management

## Running Tests

```bash
# Test a specific success case
cargo run -- build tests/cases/success/basic_declarations.bs

# Test a specific failure case (should show error)
cargo run -- build tests/cases/failure/invalid_reassignment_shadowing.bs

# Test all cases in a directory
cargo run -- build tests/cases/success
cargo run -- build tests/cases/failure

# Development testing
cargo run --features "verbose_ast_logging,verbose_eval_logging,verbose_codegen_logging,detailed_timers" -- build tests/cases/test.bs
```

## Expected Behavior

- **Success cases**: Should compile without errors and produce valid output
- **Failure cases**: Should produce specific, helpful error messages explaining what went wrong
- **Error messages**: Should be descriptive and suggest correct syntax when possible

## Recent Fixes

- **Fixed undefined variable handling**: The compiler now properly catches undefined variables during expression parsing and provides user-friendly error messages instead of throwing internal compiler errors at the MIR stage.
- **Fixed invalid reassignment detection**: Using `~=` for variable reassignment (instead of initial declaration) now properly triggers a rule error with helpful guidance.
- **Improved error classification**: Undefined variables now use `return_rule_error!` (semantic error) instead of `return_syntax_error!` (syntax error) for more accurate error categorization.