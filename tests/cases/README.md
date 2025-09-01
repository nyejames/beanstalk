# Beanstalk Test Cases

This directory contains test cases for the Beanstalk compiler, organized into success and failure cases.

## Directory Structure

### `success/` - Tests that should compile successfully
- `basic_declarations.bst` - Basic variable declarations (mutable and immutable)
- `proper_mutations.bst` - Correct mutation syntax using `=` operator
- `basic_arithmetic.bst` - Arithmetic operations and expressions
- `correct_mutation_test.bst` - Proper mutable variable usage patterns
- `declarations_only.bst` - Variable declarations without mutations
- `basic_features.bst` - Comprehensive language feature tests
- `simple_arithmetic.bst` - Simple arithmetic with proper mutation syntax
- `basic_math.bst` - Mathematical operations and function exports
- `if_statements.bst` - Conditional statement tests

### `failure/` - Tests that should trigger compiler errors
- `invalid_reassignment_shadowing.bst` - Using `~=` for reassignment (should be error)
- `undefined_variable_mutation.bst` - Attempting to mutate undefined variables
- `immutable_mutation_attempt.bst` - Trying to mutate immutable variables
- `variable_redeclaration.bst` - Variable redeclaration/shadowing attempts
- `borrow_checker_use_after_move.bst` - Use after move violations
- `borrow_checker_multiple_mutable_borrows.bst` - Multiple mutable borrow conflicts
- `borrow_checker_mutable_immutable_conflict.bst` - Mutable/immutable borrow conflicts

### `test.bst` - Development scratch file
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
cargo run -- build tests/cases/success/basic_declarations.bst

# Test a specific failure case (should show error)
cargo run -- build tests/cases/failure/invalid_reassignment_shadowing.bst

# Test all cases in a directory
cargo run -- build tests/cases/success
cargo run -- build tests/cases/failure

# Development testing
cargo run --features "verbose_ast_logging,verbose_eval_logging,verbose_codegen_logging,detailed_timers" -- build tests/cases/test.bst
```

## Expected Behavior

- **Success cases**: Should compile without errors and produce valid output
- **Failure cases**: Should produce specific, helpful error messages explaining what went wrong
- **Error messages**: Should be descriptive and suggest correct syntax when possible

## Recent Fixes

- **Fixed undefined variable handling**: The compiler now properly catches undefined variables during expression parsing and provides user-friendly error messages instead of throwing internal compiler errors at the MIR stage.
- **Fixed invalid reassignment detection**: Using `~=` for variable reassignment (instead of initial declaration) now properly triggers a rule error with helpful guidance.
- **Improved error classification**: Undefined variables now use `return_rule_error!` (semantic error) instead of `return_syntax_error!` (syntax error) for more accurate error categorization.