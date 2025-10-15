# WIR Module Organization Guide

This document provides detailed information about the WIR (WASM Intermediate Representation) module organization in the Beanstalk compiler, including the rationale for the modular design and guidelines for working with each module.

## Overview

The WIR system was refactored from a single large file (`build_wir.rs` with 3,803 lines) into a modular architecture with focused, maintainable modules. This refactoring improves code organization, enables better testing, and makes the codebase more approachable for new contributors.

## Module Structure

```
src/compiler/wir/
├── mod.rs                 # Module declarations and documentation
├── build_wir.rs          # Main entry point (~200 lines)
├── context.rs            # Context management (~800 lines)
├── expressions.rs        # Expression transformation (~900 lines)
├── statements.rs         # Statement transformation (~800 lines)
├── templates.rs          # Template transformation (~600 lines)
├── utilities.rs          # Utility functions (~400 lines)
├── wir_nodes.rs          # WIR node definitions (existing)
├── place.rs              # Place abstraction (existing)
├── wir.rs                # Core WIR structures (existing)
├── extract.rs            # Fact extraction (existing)
└── borrow_checker.rs     # Borrow checking (existing)
```

## Module Responsibilities

### Core Transformation Pipeline

#### `build_wir.rs` - Main Entry Point
**Purpose**: Orchestrates the complete AST-to-WIR transformation process

**Key Functions**:
- `ast_to_wir()`: Main transformation entry point
- `run_borrow_checking_on_wir()`: Integrated borrow checking
- `regenerate_events_for_function()`: Event generation for borrow analysis

**Dependencies**: All other WIR modules
**Size**: ~200 lines (reduced from 3,803)

#### `context.rs` - Context Management
**Purpose**: Manages transformation state, variable scoping, and place allocation

**Key Types**:
- `WirTransformContext`: Central transformation context
- `VariableUsageTracker`: Last-use analysis for move detection
- `TemporaryVariable`: Temporary variable lifecycle management
- `ExpressionStackEntry`: RPN evaluation stack management

**Key Responsibilities**:
- Variable scoping with lexical scope rules
- Place allocation for WASM locals and memory
- Temporary variable creation and cleanup
- Host function import management
- Usage tracking for Beanstalk's implicit borrowing

**Integration Points**:
- WASIX registry for system function compatibility
- WASI compatibility layer for migration support
- Place manager for WASM-aware memory allocation

#### `expressions.rs` - Expression Transformation
**Purpose**: Converts AST expressions to WIR rvalues and operands

**Key Functions**:
- `expression_to_rvalue_with_context()`: Core expression transformation
- `expression_to_operand_with_context()`: Operand-specific conversion
- `evaluate_rpn_to_wir_statements()`: RPN expression evaluation
- `ast_expression_to_wir()`: Expression statement handling

**Expression Types Handled**:
- **Literals**: Direct constant conversion
- **Variables**: Place lookup and reference creation
- **Templates**: Integration with template system
- **Runtime Expressions**: Stack-based RPN evaluation
- **Binary Operations**: Type-aware operator processing

**Dependencies**: `context.rs`, `templates.rs`, `utilities.rs`

#### `statements.rs` - Statement Transformation
**Purpose**: Converts AST statements to WIR statements

**Key Functions**:
- `transform_ast_node_to_wir()`: Main statement dispatch
- `ast_declaration_to_wir()`: Variable declarations
- `ast_mutation_to_wir()`: Variable assignments
- `ast_function_call_to_wir()`: Function calls
- `ast_host_function_call_to_wir()`: System function calls
- `ast_if_statement_to_wir()`: Control flow

**Statement Types Handled**:
- **Declarations**: Variable creation with place allocation
- **Mutations**: Variable updates with borrow checking
- **Function Calls**: Regular and host function invocations
- **Control Flow**: If statements, loops, and branches
- **Expression Statements**: Standalone expressions

**Dependencies**: `context.rs`, `expressions.rs`, `utilities.rs`

#### `templates.rs` - Template Transformation
**Purpose**: Handles Beanstalk's template system and string processing

**Key Functions**:
- `transform_template_to_rvalue()`: Template-to-string conversion
- `transform_runtime_template_to_rvalue()`: Runtime template evaluation
- `transform_template_with_variable_interpolation()`: Variable embedding
- `transform_struct_literal_to_statements_and_rvalue()`: Struct construction

**Template Features**:
- **Compile-time Templates**: Static string generation
- **Runtime Templates**: Dynamic content with function calls
- **Variable Interpolation**: Embedded variable references
- **Struct Literals**: Template-based struct construction
- **String Coercion**: Type-to-string conversion

**Dependencies**: `context.rs`, `expressions.rs`

#### `utilities.rs` - Utility Functions
**Purpose**: Provides common functions used across all WIR modules

**Key Functions**:
- `infer_binary_operation_result_type()`: Type inference for operations
- `operand_to_datatype()`: Type extraction from operands
- `datatypes_match_base_type()`: Type compatibility checking
- `ast_operator_to_wir_binop()`: Operator conversion
- `datatype_to_string()`: Type names for error messages

**Utility Categories**:
- **Type Checking**: Compatibility and inference functions
- **Type Conversion**: AST-to-WIR type mapping
- **Error Helpers**: String conversion for error messages
- **Operator Mapping**: AST operator to WIR operation conversion
- **WASM Integration**: Type alignment with WASM value types

**Dependencies**: Minimal (core compiler types only)

### Core WIR Infrastructure

#### `wir_nodes.rs` - WIR Node Definitions
**Purpose**: Defines all WIR statement, terminator, and operand types
**Status**: Existing module, enhanced with better documentation

#### `place.rs` - Place Abstraction
**Purpose**: WASM-aware memory location abstraction
**Status**: Existing module, integrated with new context management

#### `wir.rs` - Core WIR Structures
**Purpose**: Top-level WIR program representation
**Status**: Existing module, works with new modular transformation

### Borrow Checking Integration

#### `extract.rs` - Fact Extraction
**Purpose**: Extracts Polonius facts for borrow checking
**Status**: Existing module, integrated with new event generation

#### `borrow_checker.rs` - Borrow Checking
**Purpose**: Implements Polonius-style borrow checking
**Status**: Existing module, works with modular WIR generation

## Design Principles

### Single Responsibility
Each module has a clear, focused responsibility:
- **Context**: State management only
- **Expressions**: Expression transformation only
- **Statements**: Statement transformation only
- **Templates**: Template system only
- **Utilities**: Common functions only

### Clear Dependencies
Module dependencies form a clean hierarchy:
```
build_wir.rs
├── context.rs (foundational)
├── utilities.rs (foundational)
├── expressions.rs → context.rs, utilities.rs
├── statements.rs → context.rs, expressions.rs, utilities.rs
└── templates.rs → context.rs, expressions.rs
```

### API Compatibility
The refactoring maintains complete API compatibility:
- All public functions remain accessible
- Import paths unchanged through re-exports
- Function signatures preserved
- Behavior identical to original implementation

### WASM-First Design
All modules designed with WASM generation in mind:
- **Context**: Place allocation for WASM locals/memory
- **Expressions**: Type alignment with WASM value types
- **Statements**: Operations map to WASM instructions
- **Templates**: String handling for WASM linear memory
- **Utilities**: Type checking for WASM compatibility

## Development Guidelines

### Adding New Features

#### New Expression Types
1. Add parsing logic to AST (if needed)
2. Extend `expression_to_rvalue_with_context()` in `expressions.rs`
3. Add type inference logic to `utilities.rs` (if needed)
4. Add tests for the new expression type

#### New Statement Types
1. Add AST representation (if needed)
2. Extend `transform_ast_node_to_wir()` in `statements.rs`
3. Add helper functions for the specific statement type
4. Integrate with context management for variable handling
5. Add comprehensive tests

#### New Template Features
1. Extend template AST representation (if needed)
2. Add transformation logic to `templates.rs`
3. Integrate with expression system for embedded expressions
4. Add string processing utilities as needed
5. Test both compile-time and runtime template scenarios

### Testing Strategy

#### Unit Testing
- **Context Module**: Variable scoping, place allocation, temporary management
- **Expression Module**: All expression types, RPN evaluation, type inference
- **Statement Module**: All statement types, control flow, function calls
- **Template Module**: Template compilation, variable interpolation, struct literals
- **Utility Module**: Type checking, operator conversion, error message generation

#### Integration Testing
- **Cross-Module**: Expression evaluation within statements
- **Context Integration**: Variable lookup across module boundaries
- **Template Integration**: Templates within expressions and statements
- **Error Propagation**: Error handling across module boundaries

#### Regression Testing
- **API Compatibility**: All existing tests continue to pass
- **Error Messages**: Identical error output to original implementation
- **Performance**: No significant compilation time regression
- **Borrow Checking**: All lifetime analysis results unchanged

### Code Quality Standards

#### Documentation
- **Module-Level**: Clear purpose and responsibility documentation
- **Function-Level**: Comprehensive parameter and return value documentation
- **Type-Level**: Clear explanation of data structure purposes
- **Example Usage**: Code examples for complex functions

#### Error Handling
- **Consistent Patterns**: Use appropriate error macros for each error type
- **Source Locations**: Preserve precise error locations across modules
- **Helpful Messages**: Clear, actionable error messages with context
- **Error Propagation**: Proper error handling at module boundaries

#### Performance Considerations
- **Minimal Overhead**: Module boundaries should not impact performance
- **Efficient Context**: Context passing should be lightweight
- **Memory Management**: Proper cleanup of temporary variables
- **Compilation Speed**: Modular structure should not slow compilation

## Migration Benefits

### Maintainability Improvements
- **Focused Modules**: Each module has a single, clear purpose
- **Reduced Complexity**: No single file exceeds 1000 lines
- **Clear Boundaries**: Module responsibilities are well-defined
- **Easier Navigation**: Developers can quickly find relevant code

### Extensibility Enhancements
- **Incremental Development**: New features can be added to specific modules
- **Parallel Development**: Multiple developers can work on different modules
- **Feature Isolation**: Changes to one area don't affect unrelated code
- **Testing Isolation**: Module-specific tests enable focused debugging

### Code Quality Benefits
- **Better Testing**: Unit tests can focus on specific functionality
- **Improved Documentation**: Module-level documentation provides clear guidance
- **Consistent Patterns**: Each module follows established patterns
- **Error Isolation**: Bugs are confined to specific modules

### Future Extensibility
- **New Language Features**: Can be added to appropriate modules
- **Performance Optimizations**: Module-specific optimizations possible
- **Alternative Backends**: Modular structure supports future backend additions
- **Tooling Integration**: Better IDE support with focused modules

This modular architecture ensures that the WIR system remains maintainable and extensible as the Beanstalk compiler continues to evolve, while preserving all existing functionality and maintaining the high performance standards required for efficient WASM generation.