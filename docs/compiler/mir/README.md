# Beanstalk MIR Documentation

This directory contains comprehensive documentation for Beanstalk's Mid-level Intermediate Representation (MIR) and its simplified dataflow-based borrow checking system.

## Documents

### Core Documentation

- **[MIR Refactor Guide](mir-refactor-guide.md)** - Complete overview of the simplified MIR architecture, design principles, and key components
- **[Dataflow Analysis Guide](dataflow-analysis-guide.md)** - Detailed documentation of the backward liveness and forward loan-liveness dataflow algorithms
- **[Migration Guide](migration-guide.md)** - Step-by-step guide for migrating from the old Polonius-style system to the new dataflow-based system

### Examples

- **[Borrow Checking Examples](examples/borrow-checking-examples.md)** - Comprehensive examples showing the borrow checking system in action, including MIR events, dataflow analysis, and conflict detection

## Quick Overview

The simplified MIR system replaces complex Polonius-style constraint solving with standard dataflow analysis, providing:

- **Simplicity**: Simple events instead of complex facts
- **Performance**: Linear scaling instead of quadratic/cubic growth
- **Maintainability**: Standard algorithms that are well-understood
- **Precision**: Field-sensitive aliasing with precise last-use detection

## Architecture

```
AST → MIR Lowering → Liveness Analysis → Loan Dataflow → Conflict Detection → WASM
     (3-address)    (backward)         (forward)      (aliasing)
```

### Key Components

- **Program Points**: Sequential identifiers for each MIR statement
- **Events**: Simple borrow events (StartBorrow, Use, Move, Drop) per program point  
- **Dataflow Analysis**: Standard forward/backward algorithms with efficient bitsets
- **Conflict Detection**: Precise aliasing-based borrow conflict detection

## Getting Started

1. Start with the [MIR Refactor Guide](mir-refactor-guide.md) for a complete overview
2. Review the [Examples](examples/borrow-checking-examples.md) to see the system in action
3. Consult the [Dataflow Analysis Guide](dataflow-analysis-guide.md) for algorithm details
4. Use the [Migration Guide](migration-guide.md) when updating existing code

## Implementation

The MIR system is implemented in `src/compiler/mir/` with the following key modules:

- `mir_nodes.rs` - Core MIR types and data structures
- `build_mir.rs` - AST to MIR lowering with event generation
- `liveness.rs` - Backward liveness analysis for last-use refinement
- `dataflow.rs` - Forward loan-liveness dataflow analysis
- `check.rs` - Borrow conflict detection and aliasing analysis
- `diagnose.rs` - User-friendly error diagnostics

See the individual documentation files for detailed information about each component.