# Beanstalk `as` keyword general renaming implementation plan

## Goal

Extend `as` from type aliases and partial import aliasing into the compiler-owned general renaming keyword for exactly these domains:

```beanstalk
UserId as Int
import @models/User as DocsUser
import @components {render as render_component, Button as UiButton}
case Variant(original_name as local_name) => ...
```

`as` must remain rejected everywhere else with a targeted diagnostic.

The implementation should keep aliases transparent and file-local. Import aliases are not re-exported by the importing file. A file that wants to expose a public type alias must declare a real exported type alias explicitly, for example:

```beanstalk
import @models/User as ExternalUser
# DocsUser as ExternalUser
```

That creates a new exported type alias declaration. It does not re-export the import alias itself.

## Design decisions from interview

- `as` import aliases apply to all explicit imported symbols: functions, constants, structs, choices, type aliases, and external package symbols.
- Grouped imports support per-entry aliases.
- Grouped nested entries such as `pages/home/render as render_home` rename only the local binding; the canonical target remains the resolved full symbol path.
- Alias collisions are hard errors. Import aliases must not collide with same-file declarations, imported symbols, prelude symbols, builtins, or visible type aliases.
- Internally the compiler can keep separate maps for values, types, externals, and type aliases, but user-facing duplicate visible spelling should be rejected.
- Alias naming convention is warning-only for now. Do not perform full kind analysis. Compare only the first character case of the imported symbol spelling and alias spelling. If one starts uppercase and the other does not, emit a warning.
- Import aliases remain file-local only.
- Type aliases may target imported aliases.
- Duplicate alias diagnostics should list both import locations if the previous source span is cheaply available. Otherwise report at the second alias.
- Choice payload alias syntax is constructor-like: `case Variant(original_name as local_name) =>`.
- Choice payload aliasing is part of this plan, but in later phases after import aliasing.
- Roadmap/matrix entries should keep source import aliases, external import aliases, grouped import aliases, and Choice payload aliases separate.
- Phase shape should be source imports, external imports, grouped imports, then payload renaming, with audit commits after import phases, after payload phases, and at the end.

## Current repo state to anchor against

The existing compiler already has partial single-import alias plumbing:

- `src/compiler_frontend/paths/const_paths.rs`
  - `ParsedImportItem` currently stores `path` and `alias`.
  - `parse_import_clause_items` supports a trailing `as alias` after a single path.
  - It explicitly rejects one trailing alias after a grouped import with: grouped imports cannot use a single trailing alias; use per-entry aliases instead.
  - `TokenKind::Path(Vec<InternedPath>)` currently loses per-entry alias metadata once grouped paths are expanded.
- `src/compiler_frontend/headers/types.rs`
  - `FileImport` stores `header_path`, `alias`, and one `location`.
  - It does not currently preserve an alias-specific location or grouped-entry-specific location.
- `src/compiler_frontend/headers/file_parser.rs`
  - Header parsing calls `parse_import_clause_items`, normalizes each path, inserts the local name into `encountered_symbols`, and stores `FileImport` records.
- `src/compiler_frontend/ast/import_bindings.rs`
  - AST import binding resolves source imports, type aliases, and external package symbols.
  - `FileImportBindings` already separates `visible_symbol_paths`, `visible_external_symbols`, `visible_source_bindings`, and `visible_type_aliases`.
  - The binder currently uses `import.alias.unwrap_or(symbol_name)` for source symbols and external symbols.
  - Collision logic exists but should be tightened into a unified user-visible name registry.
- `src/compiler_frontend/ast/statements/match_patterns.rs`
  - Choice payload capture patterns already use constructor-like syntax: `case Variant(field)`.
  - Rename syntax is currently rejected as deferred when `as` appears after a capture name.
  - `ChoicePayloadCapture` has `field_name`, `field_index`, `field_type`, `location`, and `binding_path`.
- `src/compiler_frontend/ast/statements/branching.rs`
  - `build_arm_scope_with_captures` currently binds captures using `capture.field_name`.
- `src/compiler_frontend/hir/hir_statement/control_flow.rs`
  - HIR lowering consumes capture `binding_path` and `field_index`; it should need only small changes once AST stores original field name and local binding name separately.
- `docs/src/docs/progress/#page.bst`
  - Currently marks Choice payload matching as supported with original field names only.
  - Currently marks Choice binding renames as deferred and notes an older future syntax direction that must be corrected.

## Non-goals

- Do not add import re-export syntax.
- Do not allow aliases to shadow local declarations, imports, prelude symbols, builtins, or type aliases.
- Do not add wildcard imports.
- Do not add namespace imports.
- Do not add generic destructuring or non-choice capture renaming.
- Do not add nested payload pattern support.
- Do not make `as` a general expression cast/operator.
- Do not preserve old APIs through compatibility wrappers. Update the current data shape directly.

---

# Phase 1 — Source import aliases: make current single-import aliasing explicit and strict

## Summary

Source import aliases already mostly work structurally, but the behavior should become deliberate, tested, and strict. This phase should harden single-symbol source imports before grouped and external aliases are expanded.

The main design principle: an import alias creates one file-local visible name pointing at one canonical exported source declaration path. The canonical path remains unchanged. Only lookup spelling changes inside the importing file.

## Target syntax

```beanstalk
import @utils/render as render_utils
import @models/User as DocsUser

page = render_utils()
user DocsUser = DocsUser("Nye")
```

## Implementation steps

### 1. Tighten `FileImport` source location data

Update `src/compiler_frontend/headers/types.rs`:

```rust
pub struct FileImport {
    pub header_path: InternedPath,
    pub alias: Option<StringId>,
    pub location: SourceLocation,
    pub path_location: SourceLocation,
    pub alias_location: Option<SourceLocation>,
}
```

Rationale:

- `location` can stay as the import-clause location for broad diagnostics.
- `path_location` points to the imported path or grouped entry.
- `alias_location` enables better duplicate alias diagnostics and convention warnings.

If adding both `path_location` and `alias_location` creates too much churn, use only `name_location: SourceLocation`, where `alias_location.unwrap_or(path_location)` is stored. The important requirement is that duplicate alias diagnostics can point at the alias when available.

### 2. Extend parsed import item metadata

Update `src/compiler_frontend/paths/const_paths.rs`:

```rust
pub struct ParsedImportItem {
    pub path: InternedPath,
    pub alias: Option<StringId>,
    pub path_location: SourceLocation,
    pub alias_location: Option<SourceLocation>,
}
```

For now, single-path imports should populate:

- `path_location` from the `TokenKind::Path` token location.
- `alias_location` from the alias token location.

Grouped per-entry locations will be completed in Phase 3.

### 3. Thread location metadata through header parsing

Update `src/compiler_frontend/headers/file_parser.rs`:

- When creating `FileImport`, copy the new location fields from `ParsedImportItem`.
- Keep `location` as the broad import keyword/current location unless diagnostics are clearly better with `path_location`.
- Insert `item.alias.or_else(|| normalized_path.name())` into `encountered_symbols` as today, but use the normalized path only for the fallback name.

### 4. Introduce a unified import binding name registry

Update `src/compiler_frontend/ast/import_bindings.rs`.

Replace the ad-hoc `bound_names: FxHashSet<StringId>` checks with a small helper structure:

```rust
enum VisibleNameKind {
    SameFileDeclaration,
    SourceImport,
    TypeAliasImport,
    ExternalImport,
    PreludeExternal,
    Builtin,
}

struct VisibleNameBinding {
    kind: VisibleNameKind,
    canonical_path: Option<InternedPath>,
    external_symbol_id: Option<ExternalSymbolId>,
    location: Option<SourceLocation>,
}

struct VisibleNameRegistry {
    names: FxHashMap<StringId, VisibleNameBinding>,
}
```

Keep this private to `import_bindings.rs` unless it becomes useful elsewhere.

Responsibilities:

- Register same-file declarations before imports.
- Register builtins and prelude names before imports so aliases cannot collide with them.
- Register source imports, type alias imports, and external imports through the same collision check.
- Produce consistent diagnostics.
- Allow exact duplicate import of the same canonical symbol only if the local name also maps to the same canonical target and no alias was used. If this compatibility is not needed, reject all duplicate imports for strictness.

Recommendation: reject all duplicate visible spellings for now. It is simpler and matches the “no duplicate visible spelling” rule.

### 5. Collision diagnostics

Implement helper:

```rust
fn report_visible_name_collision(
    local_name: StringId,
    new_location: SourceLocation,
    previous: &VisibleNameBinding,
    string_table: &StringTable,
) -> CompilerError
```

Diagnostic rules:

- If `previous.location` exists, include metadata or message text for both locations if the error system supports it cleanly.
- If not, report at `new_location`:

```text
Import name collision: 'render' is already visible in this file.
```

Suggested metadata:

- `CompilationStage => "Import Binding"`
- `ConflictType => "ImportNameCollision"`
- `VariableName => <local name>`
- `PrimarySuggestion => "Use a different import alias with `as`, or rename the existing declaration."`

For same-file declarations, existing API currently passes only `declared_names_by_file`, not declaration locations. Do not widen the whole module-symbol API just for this unless cheap. In that case, use the second import span only.

### 6. Naming-convention warning for aliases

Add warning-only behavior:

```beanstalk
import @utils/render as Render       -- warning: lower → upper
import @models/User as user          -- warning: upper → lower
```

Rules:

- Only run when `alias.is_some()`.
- Compare first character case of imported symbol name and alias name.
- Do not inspect full `snake_case` vs `camelCase`.
- Do not perform kind analysis.
- Use the resolved source symbol name for source imports.
- For external imports, use the package symbol name.
- Do not warn for names whose first character is not alphabetic.

Likely file:

- `src/compiler_frontend/compiler_messages/compiler_warnings.rs` or current warning module naming.

Suggested warning kind:

```rust
WarningKind::ImportAliasCaseMismatch
```

Suggested text:

```text
Import alias 'Render' uses different leading-name case than imported symbol 'render'.
```

### 7. Ensure type aliases can target import aliases

Audit type resolution after source alias binding:

- `visible_type_aliases` must include aliased imported type aliases.
- `visible_source_bindings` must include aliased imported structs/choices so named type resolution can resolve through the alias in type positions.
- `resolve_named_types_in_data_type` call sites should resolve `NamedType(alias)` through visible alias maps, not through global declaration names.

Add or update tests to prove:

```beanstalk
import @models/User as ExternalUser
LocalUser as ExternalUser
```

This should work if `ExternalUser` points to an imported struct/choice/type alias.

## Tests

Add integration cases under `tests/cases/` and register them in `tests/cases/manifest.toml`.

Suggested cases:

1. `source_import_function_alias_success`
   - Import an exported function with `as`.
   - Call only the alias.
   - Verify output.

2. `source_import_constant_alias_success`
   - Import an exported constant with `as`.
   - Use it in a const/template context and runtime expression if applicable.

3. `source_import_type_alias_success`
   - Import a struct or choice as an alias.
   - Use alias in declaration type annotation and constructor call.

4. `type_alias_targets_import_alias_success`
   - Import a type as `ExternalUser`.
   - Declare `# User as ExternalUser` or local `User as ExternalUser`.
   - Use `User` transparently.

5. `source_import_alias_collision_with_local_rejected`
   - Same-file declaration `render` plus `import @x/render as render`.
   - Expect rule error.

6. `source_import_alias_duplicate_rejected`
   - Two imports alias to same local name.
   - Expect collision error.

7. `source_import_alias_visible_only_in_file_rejected`
   - File A imports `@models/User as DocsUser` but does not declare a type alias.
   - File B attempts to import or use `DocsUser` through File A.
   - Expect missing import target or unknown type.

8. `source_import_alias_case_warning`
   - Alias `render as Render` or `User as user`.
   - Expect warning, not failure.

## Phase 1 commit structure

1. `import-alias-data-model`
   - Add source/alias location fields.
   - Thread through parsed import items and file imports.

2. `source-import-alias-binding`
   - Add unified visible-name registry.
   - Harden collision behavior for source declarations and type aliases.

3. `source-import-alias-tests`
   - Add integration cases and warning coverage.

---

# Phase 2 — External package import aliases

## Summary

External packages already resolve into `visible_external_symbols` keyed by a local name. This phase makes external alias behavior explicit and gives it the same collision and warning semantics as source imports.

External aliases must work for:

- external functions
- external constants
- external opaque types

## Target syntax

```beanstalk
import @std/math/sin as sine
import @std/math/PI as pi_value

angle Float = 1.0
value = sine(angle) + pi_value
```

Opaque external types should also alias correctly where package metadata provides them:

```beanstalk
import @web/canvas/Canvas as DrawingCanvas
```

## Implementation steps

### 1. Reuse the visible-name registry

In `src/compiler_frontend/ast/import_bindings.rs`:

- Resolve virtual package imports as today.
- Compute `local_name = import.alias.unwrap_or(symbol_name)`.
- Register `local_name` through the same `VisibleNameRegistry` used for source imports.
- Reject collision with source declarations, source imports, type alias imports, external imports, prelude names, and builtins.

Important behavior change:

- Current prelude injection happens after explicit imports and skips if a name already exists.
- The requested design says import aliases must not collide with prelude symbols.
- Preload prelude names into the registry before explicit imports, then inject prelude symbols into `visible_external_symbols` after import processing only for names that were not user-declared/imported.
- If same-file declarations already block prelude declarations in header parsing, keep that behavior. The import binder still needs to reject import aliases like `as io`.

### 2. External constants audit

Current `resolve_virtual_package_import` must be checked for constants.

Make sure a package constant can be found by the same package-symbol lookup path as functions/types. If the registry has distinct function/type/constant lookup methods, `VirtualPackageMatch::Found` should consider constants too.

Acceptance criteria:

```beanstalk
import @std/math/PI as pi
# circumference = pi * 2.0
```

works as a compile-time constant path if `PI` is registry-provided as a compile-time scalar constant.

### 3. External alias warning

Run the same first-character case warning for external aliases.

Examples:

```beanstalk
import @std/math/sin as Sine   -- warning
import @pkg/Canvas as canvas   -- warning
```

### 4. Prelude collision tests

Add negative tests:

```beanstalk
import @std/math/sin as io
```

Expected result: hard error, because `io` is prelude-visible.

Also test any current prelude type name such as `IO`:

```beanstalk
import @models/IO as IO
```

If `IO` is prelude-visible in the file, this must be a collision.

## Tests

Suggested cases:

1. `external_import_function_alias_success`
   - `import @std/math/sin as sine`
   - Use `sine(1.0)`.

2. `external_import_constant_alias_success`
   - `import @std/math/PI as pi`
   - Use in const and runtime contexts.

3. `external_import_group_alias_deferred_until_phase_3`
   - If grouped alias parsing is still not implemented in this phase, keep grouped external alias syntax rejected with a structured diagnostic.
   - This test can be removed or rewritten in Phase 3.

4. `external_import_alias_collision_with_source_rejected`
   - Declare/import source `sine`, then external alias `as sine`.

5. `external_import_alias_collision_with_prelude_rejected`
   - Alias to `io` or `IO`.

6. `external_import_alias_case_warning`
   - Warning-only case mismatch.

## Phase 2 commit structure

1. `external-import-alias-binding`
   - Use shared registry for external imports.
   - Preload prelude names for collision checks.
   - Audit constants.

2. `external-import-alias-tests`
   - Add success, collision, and warning cases.

---

# Phase 3 — Grouped import aliases

## Summary

Grouped aliases are the main parser/data-shape gap. Current path tokenization expands grouped paths into `Vec<InternedPath>`, which loses per-entry alias data. This phase should make grouped import entries first-class enough to support aliases without creating a second import parser that fights tokenization.

## Target syntax

```beanstalk
import @components {
    render as render_component,
    Button as UiButton,
    pages/home/render as render_home,
}

import @std/math {
    PI as pi,
    sin as sine,
    cos,
}
```

Invalid syntax:

```beanstalk
import @components {render, Button} as ui
```

This remains invalid because aliases are per imported symbol, not group-level.

## Parser design options

### Recommended option: change path token payload to preserve import item metadata

Update `TokenKind::Path` from:

```rust
Path(Vec<InternedPath>)
```

to a richer shape:

```rust
Path(Vec<PathTokenItem>)

pub struct PathTokenItem {
    pub path: InternedPath,
    pub alias: Option<StringId>,
    pub path_location: SourceLocation,
    pub alias_location: Option<SourceLocation>,
}
```

Then provide path-only helpers for non-import consumers:

```rust
impl PathTokenItem {
    pub fn path_only(&self) -> InternedPath { ... }
}
```

or helper functions:

```rust
fn path_token_paths(items: &[PathTokenItem]) -> Vec<InternedPath>
```

Why this is best:

- It avoids reparsing already-tokenized source.
- It keeps aliases attached to the entries that produced them.
- It lets grouped entry locations be accurate.
- It avoids a fragile special parser in header parsing.

Cost:

- All `TokenKind::Path` pattern matches must be updated.
- This is acceptable because the style guide prefers one current API shape, not compatibility layers.

### Alternative option: add `TokenKind::ImportPath(Vec<ParsedImportItem>)`

This keeps normal paths simpler but makes lexer output context-sensitive. Only choose this if the lexer already tracks “currently tokenizing an import clause” cleanly. Do not add ad-hoc previous-token checks to the lexer.

## Implementation steps

### 1. Add path-token item data shape

Likely files:

- `src/compiler_frontend/tokenizer/tokens.rs`
- `src/compiler_frontend/paths/const_paths.rs`
- path parser tests under `src/compiler_frontend/paths/tests/` or current test module paths.

Suggested shape:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathTokenItem {
    pub path: InternedPath,
    pub alias: Option<StringId>,
    pub path_location: SourceLocation,
    pub alias_location: Option<SourceLocation>,
}
```

If `TokenKind` derives become annoying because `SourceLocation` equality is not available or too broad, keep source spans outside `TokenKind::Path` and store only alias data in `ParsedImportItem`. But prefer preserving spans if the existing token model permits it.

### 2. Teach grouped path parsing about `as`

Update `parse_grouped_block` and related helpers in `src/compiler_frontend/paths/const_paths.rs`.

New internal structures:

```rust
type RelativePathExpansions = Vec<GroupedPathExpansion>;

struct GroupedPathExpansion {
    components: Vec<StringId>,
    alias: Option<StringId>,
    path_location: SourceLocation,
    alias_location: Option<SourceLocation>,
}
```

Parse rules:

- In grouped entry context, stop the path component parser at `as` as well as `,`, `}`, or nested `{`.
- After parsing a grouped leaf entry, accept optional `as Symbol`.
- Alias applies only to leaf entries.
- Nested groups can contain aliases at their leaves:

```beanstalk
import @docs {
    pages {
        home/render as render_home,
        about/render as render_about,
    },
}
```

- Reject alias on a non-leaf group prefix:

```beanstalk
import @docs {pages as p {home/render}}
```

with a structured syntax error.

### 3. Preserve grouped-entry alias through `parse_import_clause_items`

Once the path token carries item aliases, `parse_import_clause_items` should:

- Read per-path aliases from `PathTokenItem`.
- Still allow trailing `as alias` only if the path token contains exactly one path and no per-entry alias.
- Reject group-level trailing alias with a clearer diagnostic:

```text
Grouped imports cannot use a group-level alias. Add `as ...` to each grouped entry that needs renaming.
```

- Reject double alias:

```beanstalk
import @x {foo as bar} as baz
```

or:

```beanstalk
import @x/foo as bar as baz
```

### 4. Update all path-token consumers

Search all `TokenKind::Path` matches.

Expected affected areas:

- import parsing
- project config parsing
- template/path expression parsing, if any
- path tests
- rendered path usage code

For non-import path contexts, reject aliases with a syntax/rule error:

```beanstalk
# root_folders = { @lib as library }
```

Diagnostic:

```text
Path aliases are only valid in import clauses.
```

This keeps `as` domain-limited.

### 5. Source and external grouped alias binding

No major binder changes should be needed if Phase 1/2 unified the import binder correctly. Grouped imports should arrive as multiple `FileImport` entries with independent aliases and locations.

Acceptance examples:

```beanstalk
import @components {render as render_component, Button as UiButton}
import @std/math {PI as pi, sin as sine, cos}
```

All entries should resolve exactly like equivalent separate import lines.

## Tests

Unit/path parser tests:

1. `parse_grouped_import_entries_with_aliases`
2. `parse_nested_grouped_import_entries_with_aliases`
3. `reject_group_level_alias`
4. `reject_double_alias`
5. `reject_alias_in_non_import_path_context`

Integration tests:

1. `grouped_source_import_alias_success`
2. `grouped_nested_source_import_alias_success`
3. `grouped_external_import_alias_success`
4. `grouped_import_alias_collision_rejected`
5. `grouped_import_alias_case_warning`

## Phase 3 commit structure

1. `path-token-items-for-import-aliases`
   - Update `TokenKind::Path` payload and path-only helpers.
   - Update non-import consumers.

2. `grouped-import-alias-parser`
   - Parse per-entry aliases and preserve source spans.
   - Reject group-level/double aliases.

3. `grouped-import-alias-tests`
   - Add parser and integration tests.

---

# Audit A — Import alias implementation audit

This audit should be its own commit after Phases 1–3.

## Scope

Audit all import-alias work before moving to Choice payload renaming.

## Checklist

### Behavior

- Single source import aliases work.
- Single external import aliases work.
- Grouped source import aliases work.
- Grouped external import aliases work.
- Nested grouped aliases work.
- Import aliases remain file-local.
- Type aliases can target import aliases.
- Alias collisions are hard errors across source imports, external imports, type alias imports, same-file declarations, prelude symbols, and builtins.
- Prelude symbols cannot be stolen by an explicit import alias.
- Warning-only case mismatch behavior works consistently.
- `as` in non-import path contexts is rejected.

### Code shape

- `src/compiler_frontend/paths/const_paths.rs` still owns path syntax only; it must not become an import-binding module.
- `src/compiler_frontend/headers/file_parser.rs` only collects import entries; it must not resolve semantic symbol kinds.
- `src/compiler_frontend/ast/import_bindings.rs` owns semantic import resolution and collision checks.
- No duplicated alias parsing logic exists between header parsing and path parsing.
- No user-input `panic!`, `todo!`, or unsafe `unwrap()` paths were added.
- New comments explain why grouped alias metadata must be preserved through tokenization.

### Test shape

- Integration tests cover source/external/grouped/collision/file-local behavior.
- Warning tests assert warning presence where the test framework supports it.
- Negative tests assert `Rule` or `Syntax` errors as appropriate, not generic compiler errors.
- Test names are added to `tests/cases/manifest.toml`.

## Validation commands

Run:

```bash
cargo fmt --check
cargo clippy
cargo test
cargo run tests
cargo run --features "detailed_timers" docs
cargo run --release --features "detailed_timers" speed-test.bst
```

Or, if available:

```bash
just validate
```

## Audit A commit

Suggested commit:

```text
audit import alias implementation
```

---

# Phase 4 — Documentation and progress matrix for import aliases

## Summary

Once import aliases are supported, update documentation immediately before payload aliasing starts. This avoids the docs drifting while the same keyword is being extended in a separate domain.

## Documentation files

### `docs/language-overview.md`

Update:

- Syntax summary for `as` domains.
- Type alias section.
- Module System and Imports section.
- External platform package imports section.
- Add grouped alias examples.
- Add file-local rule.
- Add collision rule.
- Add case-mismatch warning note.
- Clarify that import aliases are not re-exported.

Suggested import docs:

```beanstalk
-- Import one exported symbol with its original name
import @path/to/file/symbol

-- Import with a file-local alias
import @path/to/file/symbol as local_name

-- Grouped imports can alias individual entries
import @components {
    render as render_component,
    Button as UiButton,
    Card,
}

-- Nested grouped entries can alias the final imported symbol
import @docs {
    pages/home/render as render_home,
}
```

Rules to document:

- Imports target exported symbols, not files.
- Alias applies only in the importing file.
- Alias does not change the canonical declaration path.
- Alias cannot collide with any visible name in the same file.
- Aliases should preserve the leading-case convention of the imported symbol; mismatches warn.
- Type aliases can target imported aliases.

### `docs/compiler-design-overview.md`

Update import visibility section:

- `visible_source_bindings`: local visible name → canonical source declaration path.
- `visible_type_aliases`: local visible alias/type name → canonical type alias path.
- `visible_external_symbols`: local visible name → resolved external symbol ID.
- Mention unified user-visible name collision checks during AST import binding.
- Mention grouped import expansion preserves per-entry alias metadata from tokenization/header parsing.

### `docs/src/docs/progress/#page.bst`

Update rows:

- `Paths and imports`
  - Add grouped aliases, nested grouped aliases, file-local aliases, and collision behavior to watch points/coverage.
- `External platform packages`
  - Add external function/type/constant aliasing to current support.
- `Standard math package`
  - Keep existing direct/grouped/aliased import coverage if still accurate; otherwise update to point at new canonical cases.
- Add or update deferred/reserved section rows as separate entries:
  - `Import re-exports` — Deferred or Rejected/Reserved; aliases remain file-local.
  - `Namespace / wildcard imports` — Deferred if mentioned anywhere.

Do not merge source, external, grouped, and payload alias statuses into one vague row. Keep them visible.

## Phase 4 commit structure

1. `document import aliases`
   - Update language and compiler docs.

2. `update import alias progress matrix`
   - Update `docs/src/docs/progress/#page.bst` and any matrix references.

---

# Phase 5 — Choice payload alias data model and parser

## Summary

Choice payload renaming should be implemented after import aliases are stable. It touches match-pattern parsing and local binding semantics, not import visibility.

Current supported syntax binds payload fields by their original declared field names:

```beanstalk
case Variant(original_name) =>
```

This phase adds:

```beanstalk
case Variant(original_name as local_name) =>
```

The original field name selects the payload field. The local name becomes the binding visible in the guard and arm body.

## Implementation steps

### 1. Split payload field identity from binding name

Update `src/compiler_frontend/ast/statements/match_patterns.rs`:

```rust
pub struct ChoicePayloadCapture {
    pub field_name: StringId,
    pub binding_name: StringId,
    pub field_index: usize,
    pub field_type: DataType,
    pub location: SourceLocation,
    pub binding_location: SourceLocation,
    pub binding_path: Option<InternedPath>,
}
```

Rules:

- `field_name` is the declared payload field name.
- `binding_name` is the local variable introduced in the match arm.
- Without `as`, `binding_name == field_name`.
- `location` points at the original field name.
- `binding_location` points at the local binding name if aliased, otherwise the field name location.

### 2. Parse `field as binding`

Update `parse_choice_pattern_captures`.

Current behavior rejects `TokenKind::As` as deferred. Replace that with parsing:

```rust
let field_name = capture_name;
let mut binding_name = field_name;
let mut binding_location = capture_location.clone();

if token_stream.current_token_kind() == &TokenKind::As {
    token_stream.advance();
    binding_location = token_stream.current_location();
    binding_name = parse_required_symbol_after_as(...)?;
}
```

Diagnostics:

- Missing alias after `as`:

```text
Expected local binding name after `as` in choice payload pattern.
```

- Non-symbol after `as`:

```text
Choice payload alias must be a local binding name.
```

- `case Variant(field as field)` should be allowed but warned? Recommendation: allow silently. It is redundant but harmless. If you prefer stricter style, emit a warning; do not hard-error.

### 3. Validate original field name by position

Keep the current positional validation:

```beanstalk
case Variant(first, second) =>
```

must match declared payload fields by declaration order.

With aliasing:

```beanstalk
case Variant(original_name as local_name) =>
```

validate `original_name` against the expected field at that position.

Incorrect original field names remain hard errors:

```beanstalk
case Variant(wrong_name as local_name) =>
```

### 4. Duplicate checks use binding names

Update duplicate capture checks:

- Duplicate local binding names are errors:

```beanstalk
case Pair(left as value, right as value) =>
```

- Duplicate original field names are mostly already caught by positional field-name validation. Still, keep diagnostics clear if a duplicate original appears in a position where it also mismatches expected field.

### 5. No-shadowing uses binding name

Update `src/compiler_frontend/ast/statements/branching.rs`:

- `build_arm_scope_with_captures` must use `capture.binding_name` for local name collision checks.
- Error locations should use `capture.binding_location`.
- `binding_path` should be built from `binding_name`.

Current logic:

```rust
let capture_name = capture.field_name;
```

should become:

```rust
let capture_name = capture.binding_name;
```

### 6. HIR lowering audit

Update `src/compiler_frontend/hir/hir_statement/control_flow.rs`:

- `register_match_arm_capture_locals` should use `binding_path` as today.
- Error messages that say capture field name should distinguish original field vs local binding where relevant.
- `emit_match_arm_capture_assignments` should continue using `field_index`; the alias does not affect payload extraction.
- Guard substitution should continue to map the local binding to `VariantPayloadGet` by `field_index`.

Expected minimal HIR change:

- Mostly diagnostic text and any `capture.field_name` display used for local names.
- No semantic lowering change if `binding_path` is correctly set in AST.

## Tests

Add integration tests:

1. `choice_payload_match_rename_success`

```beanstalk
Response ::
    Err |message String|,
    Success |value String|,
;

response = Response::Err("bad")

if response is:
    case Err(message as error_message) => io(error_message)
    else => io("ok")
;
```

2. `choice_payload_match_rename_guard_success`

```beanstalk
case Err(message as error_message) if error_message.is_empty() => ...
```

If string methods are not suitable, use an `Int` payload and a numeric guard.

3. `choice_payload_match_rename_wrong_original_rejected`

```beanstalk
case Err(text as error_message) =>
```

Expected: original field name mismatch.

4. `choice_payload_match_rename_duplicate_binding_rejected`

```beanstalk
case Pair(left as value, right as value) =>
```

Expected: duplicate capture binding.

5. `choice_payload_match_rename_shadowing_rejected`

Outer variable:

```beanstalk
error_message = "outer"
case Err(message as error_message) =>
```

Expected: no-shadowing error.

6. `choice_payload_match_rename_missing_alias_rejected`

```beanstalk
case Err(message as) =>
```

Expected: syntax/rule error.

7. `choice_payload_match_rename_non_symbol_alias_rejected`

```beanstalk
case Err(message as 1) =>
```

Expected: syntax/rule error.

8. `choice_payload_match_original_name_still_success`

Existing original-name capture cases should continue passing:

```beanstalk
case Err(message) => io(message)
```

## Phase 5 commit structure

1. `choice-payload-capture-binding-name`
   - Split `field_name` and `binding_name`.
   - Update AST/HIR consumers to compile with old behavior.

2. `choice-payload-as-rename-parser`
   - Parse `original_name as local_name`.
   - Add diagnostics.

3. `choice-payload-as-rename-tests`
   - Add success and negative cases.

---

# Audit B — Choice payload rename implementation audit

This audit should be its own commit after Phase 5.

## Scope

Audit match-pattern payload renaming before final docs/matrix updates.

## Checklist

### Behavior

- `case Variant(field)` still works.
- `case Variant(field as local)` works.
- Guards can reference `local`.
- Arm bodies can reference `local`.
- The original field name is not visible when a different local alias is used.
- Wrong original field names are rejected.
- Duplicate local binding names are rejected.
- Local aliases cannot shadow existing visible locals.
- HIR payload extraction still uses `field_index`, not alias spelling.
- Exhaustiveness remains tag-level.
- Nested payload patterns remain deferred.

### Code shape

- `ChoicePayloadCapture` clearly distinguishes source field identity from arm-local binding identity.
- AST scope injection uses `binding_name`, not `field_name`.
- HIR lowering relies on `binding_path` and `field_index`, not string matching.
- Comments explain why aliases do not affect payload extraction.
- No import alias logic was reused in match pattern parsing. These are separate domains using the same keyword.

### Validation commands

Run:

```bash
cargo fmt --check
cargo clippy
cargo test
cargo run tests
cargo run --features "detailed_timers" docs
cargo run --release --features "detailed_timers" speed-test.bst
```

or:

```bash
just validate
```

## Audit B commit

Suggested commit:

```text
audit choice payload alias implementation
```

---

# Phase 6 — Documentation and progress matrix for Choice payload aliases

## Summary

After payload renaming is implemented, update docs and matrix rows that currently say payload renames are deferred.

## Documentation files

### `docs/language-overview.md`

Update the pattern matching section:

```beanstalk
if response is:
    case Err(message) => io(message)
    case Success(value as success_text) => io(success_text)
;
```

Rules to document:

- Constructor-like payload pattern syntax is `case Variant(field1, field2) =>`.
- Payload field captures are positional and must use declared field names.
- `field as local_name` binds the payload field to a different local name.
- The alias is visible only in the guard and body of that match arm.
- The original field name is not additionally bound when aliased.
- Aliases cannot shadow existing variables.
- Nested payload patterns remain deferred.

Also update the `as` keyword summary to list exactly three domains:

```beanstalk
AliasName as ExistingType
import @path/symbol as local_name
case Variant(field as local_name) =>
```

### `docs/compiler-design-overview.md`

Update the match/HIR sections:

- AST match parsing validates original payload field names.
- AST creates arm-local bindings using `binding_name`.
- HIR extracts payloads by variant tag and field index.
- Alias spelling is a frontend binding concern only.

### `docs/src/docs/progress/#page.bst`

Update rows:

- `Pattern matching`
  - Rename captures should move from deferred to supported for Choice payload patterns only.
  - Keep general capture/tagged patterns deferred.
  - Keep nested payload patterns deferred.
- `Choice payload matching`
  - Add rename syntax coverage cases.
  - Update notes from “original field names only” to “original field names or `field as local_name` aliases”.
- `Choice binding renames`
  - Change status from Deferred to Supported for Choice payload captures.
  - Correct future syntax note from old syntax to `case Variant(original_name as local_name) =>`.
  - If keeping a deferred row, rename it to `General pattern binding renames` and keep it Deferred.

## Phase 6 commit structure

1. `document choice payload aliases`
   - Update language/compiler docs.

2. `update choice alias progress matrix`
   - Update progress matrix and coverage references.

---

# Phase 7 — Targeted `as` rejection audit

## Summary

After all supported `as` domains are implemented, audit parser behavior everywhere else. The goal is not to build a huge diagnostic framework, but to ensure `as` never silently parses as a path component, identifier, expression operator, or malformed declaration.

## Supported domains

Only these are valid:

```beanstalk
TypeAlias as ExistingType
import @path/symbol as local_name
case Variant(field as local_name) =>
```

Everything else should fail cleanly.

## Places to check

- Expression parsing:

```beanstalk
value = x as Int
```

Should reject with a message like:

```text
`as` is not a cast operator. Use builtin casts such as Int(value) where supported.
```

- Variable declarations:

```beanstalk
value as Int = 1
```

Should reject unless this is top-level type alias syntax with no initializer.

- Function signatures:

```beanstalk
foo |value Int as Alias|:
```

Should reject.

- Paths outside import clauses:

```beanstalk
# root_folders = { @lib as library }
```

Should reject.

- Non-choice match patterns:

```beanstalk
case 1 as one =>
```

Should reject.

- Unit variant payload aliases:

```beanstalk
case Ready(value as ready_value) =>
```

Should reject because unit variants cannot have captures.

## Likely files

- `src/compiler_frontend/ast/expressions/parse_expression_dispatch.rs`
- `src/compiler_frontend/syntax_errors/expression_position.rs`
- `src/compiler_frontend/declaration_syntax/type_syntax.rs`
- `src/compiler_frontend/declaration_syntax/declaration_shell.rs`
- `src/compiler_frontend/paths/const_paths.rs`
- `src/compiler_frontend/ast/statements/match_patterns.rs`

## Tests

Add targeted negative tests for each unsupported domain above. Avoid over-testing every syntactic variant. The requirement is clear rejection, not perfect wording everywhere.

# Final review phase — all touched areas

This must be the final commit in the plan.

## Review scope

Review all touched areas together:

- tokenizer/path parsing
- header import collection
- AST import binding
- type alias resolution through import aliases
- external package visibility
- match payload parsing
- AST match arm scope injection
- HIR match payload extraction
- diagnostics and warnings
- tests and manifest
- docs and progress matrix

## Final review checklist

### Language consistency

- `as` has exactly three supported domains.
- Import aliases and payload aliases are file/scope-local renames, not semantic type changes.
- Type aliases remain transparent compile-time aliases.
- Import aliases do not create exports.
- Choice payload aliases do not alter variant layout or field identity.

### Compiler-stage separation

- Tokenization/path parsing preserves syntax data but does not resolve semantic symbols.
- Header parsing collects import entries but does not bind them semantically.
- AST import binding resolves imports and enforces visibility/collisions.
- AST match parsing validates payload capture names and creates local bindings.
- HIR lowering consumes already-resolved field indices and binding paths.

### Diagnostics

- Collisions are hard errors.
- Duplicate import alias diagnostics include both locations where cheap.
- Alias case mismatch is a warning only.
- Unsupported `as` usage is rejected with structured diagnostics.
- No user-input panic paths were introduced.

### Tests

- `tests/cases/manifest.toml` contains all new canonical cases.
- Success cases assert output or generated artifact behavior where possible.
- Failure cases assert error category and meaningful message fragments.
- Warning cases assert warnings where supported.
- Existing import, external package, and choice payload tests still pass.

### Docs

- `docs/language-overview.md` documents all supported `as` domains.
- `docs/compiler-design-overview.md` documents stage responsibilities for aliases.
- `docs/src/docs/progress/#page.bst` honestly reflects supported/deferred surfaces.
- Any roadmap plan mentioning old Choice payload alias syntax is corrected.

## Final validation

Run the full validation set:

```bash
cargo fmt --check
cargo clippy
cargo test
cargo run tests
cargo run --features "detailed_timers" docs
cargo run --release --features "detailed_timers" speed-test.bst
```

Prefer:

```bash
just validate
```

## Final commit

Suggested commit:

```text
final review for as keyword renaming support
```

---

# Roadmap and matrix updates: exact requested changes

Update `docs/src/docs/progress/#page.bst` deliberately. Do not collapse these into a single vague row.

## Core Alpha surface: `Paths and imports`

After Phases 1–4, update to mention:

- single-symbol source import aliases: Supported
- grouped source import aliases: Supported
- nested grouped source import aliases: Supported
- aliases are file-local and not re-exported
- alias collisions are hard errors
- case mismatch is a warning

Coverage should be Broad if the suggested tests are added.

## Core/standard surface: `External platform packages`

After Phases 2–4, update to mention:

- external function aliases: Supported
- external constant aliases: Supported
- external opaque type aliases: Supported, where package metadata exposes types
- grouped external aliases: Supported after Phase 3
- prelude collision rejection

Coverage should be Broad or Targeted depending on how many package/type cases exist.

## `Standard math package`

Update this row if new canonical cases replace older alias cases.

It should mention:

- direct imports
- grouped imports
- aliased imports
- aliased constants such as `PI as pi`
- aliased functions such as `sin as sine`

## Deferred/reserved: `Import re-exports`

Add a row:

- Surface: `Import re-exports`
- Status: `Deferred` or `Rejected / Reserved`
- Canonical diagnostic direction: direct re-export syntax is not supported; aliases are file-local
- Notes: public type aliases can target imported aliases, but that is a new exported type alias declaration, not import alias re-export

## Deferred/reserved: `Wildcard / namespace imports`

Add only if the docs or roadmap mention this surface already. If not, leave it out to avoid inventing roadmap noise.

## Pattern matching row

After Phases 5–6:

- Choice payload alias captures: Supported
- General capture/tagged patterns: Deferred
- Nested payload patterns: Deferred
- Wildcard case arms still unsupported if current design remains `else =>`

## Choice payload matching row

After Phases 5–6, update note from:

```text
Constructor-like patterns with original field names only.
```

to:

```text
Constructor-like patterns support declared payload field names and `field as local_name` aliases. Field matching remains positional and validated against declared field names. Exhaustiveness is tag-level.
```

Add new canonical cases:

- `choice_payload_match_rename_success`
- `choice_payload_match_rename_guard_success`
- `choice_payload_match_rename_wrong_original_rejected`
- `choice_payload_match_rename_duplicate_binding_rejected`
- `choice_payload_match_rename_shadowing_rejected`
- `choice_payload_match_rename_missing_alias_rejected`

## Choice binding renames row

Current row should be corrected.

Before implementation:

- It may remain Deferred, but future syntax must be corrected to `case Variant(original_name as local_name) =>`.

After implementation:

- Change to Supported for Choice payload captures.
- If a deferred row is still needed, rename it to `General pattern binding renames` and keep it Deferred.

---

# Risk notes

## Highest-risk implementation point

Grouped import aliases are the highest-risk part because current grouped path tokenization expands paths too early and loses per-entry metadata. Do not patch this with string re-parsing in header parsing. Fix the token/path item shape cleanly.

## Medium-risk implementation point

Prelude collision semantics may reveal existing behavior that allowed explicit imports to silently replace prelude visibility. The requested behavior is stricter: aliases/imports must not collide with prelude symbols. Add tests so this does not regress.

## Low-risk implementation point

Choice payload aliases are mostly an AST binding-name split. HIR should remain field-index based and should not need structural redesign.

## Diagnostic risk

Two-location diagnostics may not be cheap with current `CompilerError`. Implement the better message only where previous source spans are already available. Do not widen half the compiler API just to attach previous declaration locations.

---

# Suggested total commit sequence

1. `import-alias-data-model`
2. `source-import-alias-binding`
3. `source-import-alias-tests`
4. `external-import-alias-binding`
5. `external-import-alias-tests`
6. `path-token-items-for-import-aliases`
7. `grouped-import-alias-parser`
8. `grouped-import-alias-tests`
9. `audit import alias implementation`
10. `document import aliases`
11. `update import alias progress matrix`
12. `choice-payload-capture-binding-name`
13. `choice-payload-as-rename-parser`
14. `choice-payload-as-rename-tests`
15. `audit choice payload alias implementation`
16. `document choice payload aliases`
17. `update choice alias progress matrix`
18. `reject-as-outside-renaming-domains`
19. `as-keyword-negative-tests`
20. `final review for as keyword renaming support`

This is intentionally more commits than strictly necessary. Each commit should be stable enough to review independently, and the audit commits create checkpoints before moving from imports to payload matching and before final cleanup.
