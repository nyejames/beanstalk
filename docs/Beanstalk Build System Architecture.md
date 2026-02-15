# Beanstalk Build System Architecture
The primary goal of this design is to make Beanstalk projects **structurally consistent**, **backend-agnostic**, and **easy to extend** with new build systems, while keeping compilation semantics centralized and deterministic.

## Design Goals
- Define a **canonical project shape** understood by the compiler
- Decouple *semantic compilation* from *output generation*
- Allow multiple build systems (HTML, JS, Wasm, tooling) to share the same frontend
- Reduce duplication and accidental divergence between project builders
- Make new build systems easy to implement without compiler internals knowledge

## High-Level Pipeline

```
Filesystem
  ↓
Project Normalization
  ↓
core_build (per module)
  ↓
Semantic Project (HIR Modules)
  ↓
ProjectBuilder
  ↓
Output Artifacts
```

The compiler is responsible for everything up to producing **validated HIR modules**. Project builders operate purely on semantic data and metadata.

## Canonical Project Structure

Beanstalk defines a minimal, opinionated project structure:

### `#config`
- A project-level configuration file
- Always located at the project root
- Parsed and validated by the compiler
- Provides a unified configuration map for all build systems

### `#*` Files and Modules
- Any file whose name starts with `#` defines a **module root**
- Any directory containing a `#*` file is treated as a separate module
- The exact name of the file (e.g. `#page`, `#layout`, `#lib`) is preserved and interpreted by the build system
- The project builder can be aware of multiple `#` files per root, but they can only exist at the root of a module

The compiler does not assign semantic meaning to `#` file names. It only enforces structure and boundaries.

## Module Normalization
Before any build system runs, the compiler:
- Discovers all modules
- Determines module boundaries
- Resolves imports within each module
- Identifies the entry start function
- Compiles each module independently through `core_build`

Each module is lowered into **HIR**, fully type-checked and semantically validated.

## ProjectBuilder Interface

A `ProjectBuilder` receives a fully normalized semantic project:

### Inputs
1. **Modules**
   - A list of modules, each containing:
     - Compiled HIR module
     - Parent directory name
     - `#` file name that defined the module
     - Reference to the module’s entry start function

2. **Config**
   - A parsed and validated configuration object
   - Includes compiler-defined fields and a generic map for builder-specific settings
   - Builders may validate only the keys relevant to them

3. **Build Mode**
   - A global `debug` or `release` flag

### Responsibilities
Project builders:
- Decide how modules are interpreted
- Decide how output files are structured
- Select and run backend code generation
- Emit artifacts (HTML, JS, Wasm, tooling output, etc.)

Project builders do **not**:
- Parse files
- Discover modules
- Read configuration files directly
- Perform semantic compilation

## `core_build`
`core_build` is the shared semantic compilation entry point.

- Takes a fully resolved module
- Produces a complete HIR module
- Enforces all language semantics
- Emits diagnostics consistently across all build systems

This guarantees that all build systems operate on identical semantic input.

## Single-File Projects

Single-file compilation follows the same rules:
- A single `.bst` file is treated as a single-module project
- The file’s directory is the module root
- Any `#config` file is ignored, defaults are used

This keeps CLI usage, tooling, and embedded compilation consistent.

