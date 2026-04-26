# Implement `as` Aliases for Imports and Type Aliases

## Purpose

Add `as` as the general Beanstalk aliasing keyword.

Initial supported surfaces:

1. Import aliases
2. Type aliases

This should support external platform packages cleanly, especially symbol conflicts such as `io`, `print`, `Canvas`, `Color`, etc.

The implementation should preserve the current compiler stage boundaries:

```text
Tokenization -> Header Parsing -> Dependency Sorting -> AST Construction -> HIR Generation -> Borrow Validation -> Backend Lowering
```

`as` is already tokenized as `TokenKind::As`, so this is not primarily tokenizer work. It is parser, header metadata, import binding, and type-resolution work.

---

## Current repo anchors

Relevant files:

- `src/compiler_frontend/tokenizer/tokens.rs`
  - `TokenKind::As` already exists.
  - `TokenKind::continues_expression()` probably does not need `As`; aliases should be parsed in explicit syntactic positions only.

- `src/compiler_frontend/paths/const_paths.rs`
  - `parse_file_path()` currently tokenizes `@...` and grouped paths into `TokenKind::Path(Vec<InternedPath>)`.
  - `parse_import_clause_tokens()` currently returns `(Vec<InternedPath>, usize)`.
  - This loses alias data, so import alias support needs a richer parsed import item shape.

- `src/compiler_frontend/headers/types.rs`
  - `FileImport` currently stores only `header_path` and `location`.
  - It should carry optional alias metadata.

- `src/compiler_frontend/headers/file_parser.rs`
  - Parses top-level `import` statements using `parse_import_clause_tokens()`.
  - Tracks `encountered_symbols` from normalized import path names.
  - Must use the alias name when present.

- `src/compiler_frontend/ast/import_bindings.rs`
  - Builds per-file `FileImportBindings`.
  - Currently splits source visibility and external visibility into:
    - `visible_symbol_paths`
    - `visible_external_symbols`
  - Import aliases should be resolved here into the local visible name.

- `src/compiler_frontend/ast/module_ast/scope_context.rs`
  - `ScopeContext` resolves locals/source declarations by visible name and external symbols through `visible_external_symbols`.
  - Import aliases should require minimal changes here if `FileImportBindings` maps the alias name correctly.

- `src/compiler_frontend/declaration_syntax/type_syntax.rs`
  - Parses type annotations and resolves `DataType::NamedType` placeholders.
  - Type aliases should resolve through the same named-type resolution path.

- `src/compiler_frontend/headers/header_dispatch.rs`
  - Classifies top-level declarations.
  - Type alias declarations should be parsed here, likely as a new `HeaderKind`.

- `src/compiler_frontend/datatypes.rs`
  - `DataType::NamedType(StringId)` is the unresolved type placeholder.
  - Type aliases should probably not become a new runtime `DataType`; they should resolve to their target `DataType` during AST type resolution.

---

## Syntax to support

### Import aliases

Single import alias:

```beanstalk
import @std/io/io as print

print("hello")
```

Grouped import alias:

```beanstalk
import @std/io { io as print, IO as StdIO }
```

Source import alias:

```beanstalk
import @utils/math/add as sum

value = sum(1, 2)
```

External package import alias:

```beanstalk
import @std/io/io as print

print("hello")
```

### Type aliases

Recommended first syntax:

```beanstalk
UserId as Int
Names as {String}
MaybeName as String?
```

Exported type alias:

```beanstalk
# UserId as Int
```

Imported type alias:

```beanstalk
import @types/UserId

id UserId = 42
```

Aliased import of a type alias:

```beanstalk
import @types/UserId as Id

id Id = 42
```

### Non-goals for this first implementation

Do not implement expression/value casts with `as` yet.

Do not implement aliasing arbitrary local variables:

```beanstalk
x as y -- not supported
```

Do not implement module namespace imports:

```beanstalk
import @std/io as io -- not a package namespace alias yet
```

Do not implement wildcard imports:

```beanstalk
import @std/io/* -- not supported
```

Do not implement type aliases for function signatures or generic type parameters unless already trivial through existing `DataType` parsing.

---

## Design rules

### `as` introduces a local visible name

For imports, the imported symbol keeps its canonical identity, but the current file sees it under the alias.

Example:

```beanstalk
import @std/io/io as print
```

Internal model:

```text
canonical external symbol: @std/io/io
local visible name: print
```

### Aliases are file-scoped

Import aliases affect only the importing source file.

They should not rename the declaration globally and should not affect the original exported symbol name.

### Aliases participate in collision checks

This must fail:

```beanstalk
foo = 1
import @std/io/io as foo
```

This must also fail:

```beanstalk
import @std/io/io as log
import @utils/log/log as log
```

Prelude symbols must not override explicit aliases or source declarations.

### Type aliases are compile-time-only type names

A type alias does not create a runtime value, struct, or wrapper type.

```beanstalk
UserId as Int
```

means `UserId` resolves to `Int` anywhere a type annotation is expected.

It should not create a new nominal type. If nominal distinction is wanted later, that should be a separate `newtype`-like feature, not `as`.

### Type alias expansion must be cycle-checked

Reject:

```beanstalk
A as B
B as A
```

Reject direct self-alias:

```beanstalk
A as A
```

---

## Phase 1: Token/parser model for import aliases

### 1.1 Add parsed import item type

In `src/compiler_frontend/paths/const_paths.rs`, replace or supplement:

```rust
pub fn parse_import_clause_tokens(...) -> Result<(Vec<InternedPath>, usize), CompilerError>
```

with a richer form:

```rust
#[derive(Clone, Debug)]
pub struct ParsedImportItem {
    pub path: InternedPath,
    pub alias: Option<StringId>,
    pub location: SourceLocation,
}

pub fn parse_import_clause_items(
    tokens: &[Token],
    start_index: usize,
) -> Result<(Vec<ParsedImportItem>, usize), CompilerError>
```

Keep `parse_import_clause_tokens()` temporarily only if several non-import path scanners need path-only behavior. If it remains, implement it by calling `parse_import_clause_items()` and discarding aliases.

Avoid duplicate parsing logic.

### 1.2 Parse `as` after imported symbol paths

Support:

```beanstalk
import @path/to/symbol as local_name
```

Token sequence after `import` should be:

```text
Import Path(...) As Symbol(alias)
```

Implementation note:

`TokenKind::Path(Vec<InternedPath>)` currently carries already-expanded paths, so alias syntax must be parsed from the token stream after the path token.

For a single path token with multiple expanded paths from grouped syntax, do not allow one trailing alias for the whole group:

```beanstalk
import @std/io { io, IO } as std -- reject
```

Use per-entry grouped aliases instead.

### 1.3 Support aliases inside grouped imports

The current path tokenizer expands grouped paths to `Vec<InternedPath>`, losing per-entry syntax such as `io as print`.

There are two possible approaches:

#### Recommended approach

Stop treating grouped import entries as opaque `TokenKind::Path(Vec<InternedPath>)` for import alias parsing. Add a dedicated import-clause parser that reads tokens after `import` and understands:

```beanstalk
import @base { symbol as alias, other }
```

This may require changing tokenization of `@base { ... }`, because currently `parse_file_path()` consumes the whole grouped path into one token.

If changing path tokenization is too broad, defer grouped alias support and support only single import aliases first. But the goal should be grouped aliases because `import @std { print as io }` is the motivating syntax.

#### Lower-risk interim approach

Extend path tokenization to return alias metadata for grouped import contexts only. This is harder because the tokenizer should not know it is tokenizing an import unless parsing mode is threaded in.

Prefer parser-level import clause parsing over making tokenizer context-sensitive for aliases.

### 1.4 Recommended staged compromise

Implement in two steps:

1. Support single import aliases now:

```beanstalk
import @std/io/io as print
```

2. Add grouped alias support immediately after by refactoring import parsing to preserve grouped entry metadata.

Do not block all alias work on grouped import parsing if that becomes invasive.

---

## Phase 2: Carry aliases through header imports

### 2.1 Extend `FileImport`

In `src/compiler_frontend/headers/types.rs`, change:

```rust
pub struct FileImport {
    pub header_path: InternedPath,
    pub location: SourceLocation,
}
```

to:

```rust
pub struct FileImport {
    pub header_path: InternedPath,
    pub alias: Option<StringId>,
    pub location: SourceLocation,
}
```

Add helper methods:

```rust
impl FileImport {
    pub fn local_name(&self) -> Option<StringId> {
        self.alias.or_else(|| self.header_path.name())
    }
}
```

### 2.2 Update header file parsing

In `src/compiler_frontend/headers/file_parser.rs`:

- replace calls to `parse_import_clause_tokens()` with the new parsed import item API;
- normalize `item.path` with `normalize_import_dependency_path()`;
- store `alias` in `FileImport`;
- insert the alias name into `encountered_symbols` if alias exists, otherwise use normalized path name.

Current behavior inserts `normalized_path.name()` into `encountered_symbols`. With aliases, this must use the local name instead.

### 2.3 Update visible constant placeholder discovery

`discover_visible_constant_placeholders()` also scans imports.

When it creates placeholders for imported constants, the placeholder ID should use the local alias name when present.

Example:

```beanstalk
import @config/site_name as title
# page_title = title
```

The visible placeholder should allow `title` to be resolved as a constant reference in the current file.

Be careful: the placeholder must still point to the canonical dependency path for dependency analysis. If that is not possible with the current placeholder shape, add explicit alias metadata to the import bindings stage instead of overloading placeholder IDs.

---

## Phase 3: Import binding resolves aliases

### 3.1 Source declaration imports

In `src/compiler_frontend/ast/import_bindings.rs`, `resolve_file_import_bindings()` should use the import's local name:

```rust
let local_name = import.alias.or_else(|| symbol_path.name()).ok_or(...)?;
```

For source declaration imports:

- keep `visible_symbol_paths` storing the canonical symbol path;
- add a way for `ScopeContext` / `TopLevelDeclarationIndex` to resolve `local_name` to `canonical_path`.

Important: currently `TopLevelDeclarationIndex::get_visible(name, visible_paths)` indexes declarations by their canonical names. If `visible_symbol_paths` only contains canonical paths, aliasing a source declaration will not work because lookup by alias name will not find a declaration bucket.

Therefore source import aliases need an alias map.

Add to `FileImportBindings`:

```rust
pub(crate) visible_source_aliases: FxHashMap<StringId, InternedPath>
```

or more generally:

```rust
pub(crate) visible_declaration_aliases: FxHashMap<StringId, InternedPath>
```

Then in `ScopeContext::get_reference()`:

1. Check local declarations by name.
2. Check visible declaration aliases by name. If found, fetch declaration by canonical path.
3. Fall back to existing top-level visible lookup by name.

This needs a path-indexed lookup in `TopLevelDeclarationIndex`:

```rust
by_path: FxHashMap<InternedPath, u32>

pub fn get_by_path(&self, path: &InternedPath) -> Option<&Declaration>
```

Do not fake source aliasing by inserting synthetic declarations. That would corrupt nominal identity and duplicate declarations.

### 3.2 External package imports

For external imports, store the external symbol under the alias name:

```rust
bindings.visible_external_symbols.insert(local_name, ExternalSymbolId::Function(func_id));
```

Example:

```beanstalk
import @std/io/io as print
```

should produce:

```text
visible_external_symbols[print] = Function(ExternalFunctionId::Io)
```

No further `ScopeContext` changes are needed for external aliases if lookup uses the local name.

### 3.3 Collision checks

Collision checks must use the local visible name.

Rules:

- explicit alias cannot collide with a declaration in the same file;
- explicit alias cannot collide with another imported source alias;
- explicit alias cannot collide with another imported external alias;
- prelude does not override aliases or declarations;
- source imports and external imports should be allowed to import original names that would otherwise collide if aliases disambiguate them.

Examples:

```beanstalk
import @std/io/io as print
import @other/io/io as other_print
```

should be allowed.

```beanstalk
import @std/io/io as print
import @logger/print as print
```

should fail.

---

## Phase 4: Type aliases as a new header kind

### 4.1 Add `HeaderKind::TypeAlias`

In `src/compiler_frontend/headers/types.rs`:

```rust
pub enum HeaderKind {
    ...
    TypeAlias {
        target: DataType,
    },
}
```

The target is parsed as a type annotation using the existing type syntax parser.

### 4.2 Parse top-level type alias declarations

In `src/compiler_frontend/headers/header_dispatch.rs`, add dispatch for:

```beanstalk
Name as Type
# Name as Type
```

The file parser will call `create_header()` after seeing the leading `Symbol(name)`. At that point `token_stream.current_token_kind()` should be `TokenKind::As`.

Add match arm:

```rust
TokenKind::As => {
    token_stream.advance();
    let target = parse_type_annotation(token_stream, TypeAnnotationContext::SignatureReturn)?;
    kind = HeaderKind::TypeAlias { target };
}
```

Use a new context if necessary:

```rust
TypeAnnotationContext::TypeAliasTarget
```

This would improve diagnostics:

```text
Expected a type after `as` in type alias declaration.
```

### 4.3 Dependency edges for type aliases

Collect strict dependency edges for named types inside the alias target:

```beanstalk
Alias as OtherType
```

`Alias` depends on `OtherType` if `OtherType` is imported or declared elsewhere.

Use existing `for_each_named_type_in_data_type()` and `collect_named_type_dependency_edge()`.

### 4.4 Register type aliases as symbols

In `parse_file_headers.rs::build_module_symbols()`:

- register `HeaderKind::TypeAlias` as an importable/exportable type symbol;
- exported aliases should be importable with `# Alias as Type`;
- non-exported aliases are file/module-private like other declarations.

Add type aliases to duplicate top-level declaration checks automatically through existing header path/name logic.

### 4.5 AST storage for aliases

Add an AST build-state table:

```rust
resolved_type_aliases_by_path: FxHashMap<InternedPath, DataType>
```

During AST type resolution, resolve aliases after constants/struct fields or in a dedicated pass before struct/function signature resolution.

Recommended AST pass order adjustment:

```text
1. pass_import_bindings
2. pass_type_alias_resolution
3. pass_type_resolution/constants + struct fields
4. pass_function_signatures
5. build_receiver_catalog
6. pass_emit_nodes
7. finalize
```

If a separate pass is too much, integrate alias resolution at the start of current `resolve_types()`.

### 4.6 Type alias resolution semantics

Given:

```beanstalk
UserId as Int
```

store:

```text
UserId -> Int
```

Given:

```beanstalk
Names as {String}
```

store:

```text
Names -> Collection(StringSlice)
```

Given:

```beanstalk
ExternalCanvas as Canvas
```

where `Canvas` is an imported external type, store:

```text
ExternalCanvas -> DataType::External { type_id: ... }
```

### 4.7 Avoid making type aliases runtime declarations

Do not push type aliases into AST nodes as runtime statements.

They are compile-time type metadata only.

`HeaderKind::TypeAlias` should be fully consumed by AST type resolution, similar to how constants and choices may be handled before body emission.

### 4.8 Resolve aliases during named type resolution

Update the named type resolution path so `DataType::NamedType(name)` resolves in this order:

1. visible source declaration with concrete type identity;
2. visible type alias by local name;
3. visible external type by local name;
4. error.

For aliases imported under another name, this means the alias local name should work in type annotations.

### 4.9 Cycle detection

Detect cycles while resolving alias targets.

Use a small DFS state:

```rust
enum TypeAliasVisitState {
    Visiting,
    Resolved(DataType),
}
```

Reject:

```beanstalk
A as B
B as A
```

Diagnostic should name the cycle if possible:

```text
Type alias cycle detected: A -> B -> A.
```

---

## Phase 5: ScopeContext support for source aliases and type aliases

### 5.1 Source value/function aliases

Add to `ScopeContext`:

```rust
pub visible_declaration_aliases: Option<FxHashMap<StringId, InternedPath>>
```

Add builder:

```rust
with_visible_declaration_aliases(...)
```

Update child context cloning.

Update `get_reference()`:

```rust
if let Some(local) = local_declarations_by_name.get(name) { ... }
if let Some(alias_path) = visible_declaration_aliases.get(name) { return top_level_declarations.get_by_path(alias_path); }
return top_level_declarations.get_visible(name, visible_declaration_ids)
```

This supports:

```beanstalk
import @math/add as sum

sum(1, 2)
```

### 5.2 Type alias visibility

Add to `FileImportBindings`:

```rust
visible_type_aliases: FxHashMap<StringId, InternedPath>
```

or reuse `visible_declaration_aliases` if type alias declarations are also stored in the top-level declaration index.

Recommended: keep a separate alias table because type aliases are type-only and should not resolve as values.

```rust
pub(crate) visible_type_aliases: FxHashMap<StringId, InternedPath>
```

Add to `ScopeContext` or pass through type resolution functions:

```rust
visible_type_aliases: Option<FxHashMap<StringId, InternedPath>>
```

Then type resolution can map local alias name to canonical alias declaration path, and from that to resolved target type.

Do not let type aliases resolve in expression/value position.

---

## Phase 6: Diagnostics

Add precise diagnostics for these cases.

### Import alias missing name

```beanstalk
import @std/io/io as
```

Expected:

```text
Expected alias name after `as` in import.
```

### Import alias target is not symbol

```beanstalk
import @std/io/io as 123
```

Expected:

```text
Expected alias name after `as` in import.
```

### Duplicate alias

```beanstalk
import @std/io/io as log
import @other/log as log
```

Expected:

```text
Import name collision: `log` is already visible in this file.
```

### Type alias missing target

```beanstalk
UserId as
```

Expected:

```text
Expected a type after `as` in type alias declaration.
```

### Type alias cycle

```beanstalk
A as B
B as A
```

Expected:

```text
Type alias cycle detected: A -> B -> A.
```

### Type alias used as value

```beanstalk
UserId as Int
io(UserId)
```

Expected:

```text
`UserId` is a type alias and cannot be used as a value.
```

This diagnostic may require checking visible type aliases before falling through to ordinary unknown-symbol errors.

---

## Phase 7: Tests

Prefer integration tests in `tests/cases` for user-facing behavior. Use unit tests for parser helpers and cycle detection.

### Import alias tests

1. External import alias works:

```beanstalk
import @std/io/io as print
print("hello")
```

2. Prelude does not override alias/source declaration:

```beanstalk
import @std/io/io as print
print("hello")
io("world")
```

3. Non-prelude external helper remains hidden unless imported, even with alias support:

```beanstalk
__bs_collection_length({})
```

should still fail.

4. Source function import alias works:

```beanstalk
import @utils/add as sum
sum(1, 2)
```

5. Alias collision fails.

6. Missing alias name fails.

7. Grouped alias works, once implemented:

```beanstalk
import @std/io { io as print }
print("hello")
```

### Type alias tests

1. Basic alias:

```beanstalk
UserId as Int
id UserId = 42
io(id)
```

2. Collection alias:

```beanstalk
Names as {String}
names Names = {"Ada", "Grace"}
io(names.length())
```

3. Export/import alias:

```beanstalk
-- types.bst
# UserId as Int

-- main.bst
import @types/UserId
id UserId = 1
```

4. Import alias of type alias:

```beanstalk
import @types/UserId as Id
id Id = 1
```

5. External type alias:

```beanstalk
import @std/io/IO
StdIO as IO
```

6. Alias cycle fails.

7. Type alias used as value fails.

---

## Phase 8: Documentation updates

Update `docs/language-overview.md`:

- Add `as` to syntax summary as the aliasing keyword.
- Extend import syntax examples:

```beanstalk
import @path/to/file/symbol as local_name
import @std/io { io as print, IO as StdIO }
```

- Add type alias section:

```beanstalk
UserId as Int
Names as {String}
```

Clarify that type aliases do not create new nominal types.

Update `docs/compiler-design-overview.md`:

- Mention import aliases in AST import visibility.
- Mention that `visible_external_symbols` and declaration alias maps use local names while preserving canonical symbol identity.
- Mention type aliases as compile-time-only type metadata consumed before HIR.

---

## Implementation order

Recommended commit order:

1. Add parsed import item structs and single-import `as` parsing.
2. Extend `FileImport` with `alias` and thread it through header parsing.
3. Add `visible_declaration_aliases` and make source import aliases work.
4. Make external import aliases work through `visible_external_symbols` local names.
5. Add tests for import aliases and collision behavior.
6. Add `HeaderKind::TypeAlias` and parse top-level `Name as Type`.
7. Add type alias resolution table and named-type resolution integration.
8. Add alias cycle detection.
9. Add type alias tests.
10. Add grouped import alias parsing if not already included.
11. Update docs.

Do not combine all of this into one large commit unless necessary.

---

## Validation

Run:

```bash
cargo fmt --check
cargo clippy
cargo test
cargo run tests
```

Then run the project-level validation command if available:

```bash
just validate
```

For this feature, also manually check:

```bash
cargo run tests external_package_import_alias
cargo run tests type_alias
```

using the actual test IDs chosen in `tests/cases/manifest.toml`.

---

## Acceptance criteria

- `TokenKind::As` is used only in alias syntax for now.
- Single import aliases work for source imports and external package imports.
- Grouped import aliases either work or are explicitly rejected with a clear diagnostic until implemented.
- Import aliases are file-scoped and do not mutate canonical declaration identity.
- Source import aliases resolve through an alias map, not synthetic declarations.
- External import aliases resolve through `visible_external_symbols` under the local alias name.
- Type aliases parse as top-level declarations with `Name as Type`.
- Type aliases are compile-time-only and do not emit HIR/runtime nodes.
- Type aliases resolve anywhere a named type annotation is accepted.
- Type alias cycles are rejected.
- Type aliases do not create new nominal types.
- Diagnostics for missing aliases, collisions, unknown targets, and cycles are clear.
- Docs describe import aliases and type aliases.
