# Compiler Diagnostics Redesign Implementation Plan

## Summary

Redesign Beanstalk's frontend diagnostic system so compiler stages emit structured diagnostic facts instead of rendered strings.

This plan starts after the TypeId redesign is complete. Type-related diagnostics should carry `TypeId` payloads and resolve type names only at the render boundary.

The desired end state is:

```text
Frontend/compiler stages
  -> CompilerDiagnostic { kind, severity, location, labels, payload }
  -> DiagnosticBag accumulates diagnostics locally
  -> CompilerMessages owns ordered diagnostics + StringTable at boundaries
  -> renderers produce terminal/dev-server/terse output

CompilerError
  -> internal/tooling/compiler failure only
  -> immediately printed through one central helper
  -> no longer carried as normal user-source diagnostics
```

## Current State

Relevant current implementation areas:

- `src/compiler_frontend/compiler_messages/compiler_errors.rs`
  - owns `CompilerMessages`, `CompilerError`, `ErrorType`, `ErrorMetaDataKey`, and the `return_*_error!` macros.
  - `CompilerMessages` currently stores separate `errors: Vec<CompilerError>` and `warnings: Vec<CompilerWarning>`.
  - `CompilerError` currently stores `msg: String`, `SourceLocation`, `ErrorType`, and `HashMap<ErrorMetaDataKey, String>`.

- `src/compiler_frontend/compiler_messages/compiler_warnings.rs`
  - owns `CompilerWarning` and `WarningKind`.
  - warnings currently carry rendered string data.

- `src/compiler_frontend/compiler_messages/display_messages.rs`
  - owns terminal/terse rendering for current `CompilerError` and `CompilerWarning`.
  - currently derives display names from `ErrorType` and warning text from `WarningKind`.
  - currently formats metadata from string-keyed maps.

- Boundary callers to inspect during migration:
  - `src/compiler_frontend/pipeline.rs`
  - `src/projects/check.rs`
  - `src/projects/cli.rs`
  - `src/projects/dev_server/build_loop.rs`
  - `src/projects/dev_server/error_page.rs`
  - `src/build_system/build.rs`
  - backend builder result boundaries
  - integration test assertion helpers

## Goals

- Replace user-facing `CompilerError` usage with typed `CompilerDiagnostic`.
- Store diagnostics in one ordered vector instead of separate error/warning vectors.
- Keep diagnostic categories derived from `DiagnosticKind`, not stored as a redundant field.
- Make diagnostic codes stable and explicit.
- Move final string rendering to the render boundary.
- Replace string metadata maps with typed payloads.
- Support primary and secondary source labels from the start.
- Preserve source locations and interned identity until rendering.
- Make warnings first-class diagnostics with `DiagnosticSeverity::Warning`.
- Centralize immediate `CompilerError` printing for internal/tooling failures.
- Keep the implementation readable, stage-local, and free of compatibility wrappers by the final phase.

## Non-Goals

- Do not implement full JSON diagnostic output yet.
- Do not implement full LSP protocol mapping yet.
- Do not implement localization.
- Do not implement long docs-linked explanations for every diagnostic.
- Do not generate diagnostic-code documentation automatically yet.
- Do not attempt perfect parser recovery for all syntax errors in this plan.
- Do not convert the whole compiler in one giant commit.

## Design Decisions

### `CompilerDiagnostic`

A `CompilerDiagnostic` represents a user-facing language/source/config diagnostic.

It carries facts, not final prose.

```rust
pub struct CompilerDiagnostic {
    pub kind: DiagnosticKind,
    pub severity: DiagnosticSeverity,
    pub primary_location: SourceLocation,
    pub labels: Vec<DiagnosticLabel>,
    pub payload: DiagnosticPayload,
}
```

### Severity

```rust
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Note,
}
```

Diagnostics do not carry `halts_compilation`.

Compilation halt policy belongs to the stage/pipeline. A stage may collect multiple diagnostics and stop at a safe boundary when errors exist.

### Kind and Category

`DiagnosticKind` is grouped by domain.

```rust
pub enum DiagnosticKind {
    Syntax(SyntaxDiagnosticKind),
    Type(TypeDiagnosticKind),
    Rule(RuleDiagnosticKind),
    Import(ImportDiagnosticKind),
    Borrow(BorrowDiagnosticKind),
    Config(ConfigDiagnosticKind),
    DeferredFeature(DeferredFeatureDiagnosticKind),
}
```

Diagnostic category is derived:

```rust
impl DiagnosticKind {
    pub fn category(&self) -> DiagnosticCategory {
        match self {
            DiagnosticKind::Syntax(_) => DiagnosticCategory::Syntax,
            DiagnosticKind::Type(_) => DiagnosticCategory::Type,
            DiagnosticKind::Rule(_) => DiagnosticCategory::Rule,
            DiagnosticKind::Import(_) => DiagnosticCategory::Import,
            DiagnosticKind::Borrow(_) => DiagnosticCategory::Borrow,
            DiagnosticKind::Config(_) => DiagnosticCategory::Config,
            DiagnosticKind::DeferredFeature(_) => DiagnosticCategory::DeferredFeature,
        }
    }
}
```

`DiagnosticCategory` may exist for rendering and grouping, but should not be stored inside `CompilerDiagnostic`.

### Codes and Descriptors

Diagnostic codes are explicit stable strings.

```rust
pub struct DiagnosticDescriptor {
    pub code: &'static str,
    pub title: &'static str,
    pub default_severity: DiagnosticSeverity,
}
```

Recommended code ranges:

```text
BST-SYNTAX-0001
BST-TYPE-0001
BST-RULE-0001
BST-IMPORT-0001
BST-BORROW-0001
BST-CONFIG-0001
BST-DEFERRED-0001
```

Enum variant names are implementation details. Codes are user/tool contracts.

### Payloads

Use a typed payload enum, not generic string arguments.

```rust
pub enum DiagnosticPayload {
    None,

    ExpectedToken {
        expected: TokenKind,
        found: Option<TokenKind>,
    },

    UnknownName {
        name: StringId,
        namespace: NameNamespace,
    },

    TypeMismatch {
        expected: TypeId,
        found: TypeId,
        context: TypeMismatchContext,
    },

    DuplicateDeclaration {
        name: StringId,
        first_location: SourceLocation,
    },

    BorrowConflict {
        place: PlaceId,
        existing_access: BorrowAccessKind,
        requested_access: BorrowAccessKind,
    },
}
```

Type-related payloads should use `TypeId` after the TypeId redesign. Do not store rendered type names in diagnostics.

### Labels

Support labels from the start.

```rust
pub struct DiagnosticLabel {
    pub location: SourceLocation,
    pub style: DiagnosticLabelStyle,
    pub message: Option<DiagnosticLabelMessage>,
}

pub enum DiagnosticLabelStyle {
    Primary,
    Secondary,
}
```

Label messages should eventually be typed:

```rust
pub enum DiagnosticLabelMessage {
    PreviousDeclaration,
    ExistingBorrow,
    ExpectedTypeDeclaredHere,
    ValueMovedHere,
}
```

Temporary string-backed label messages are acceptable only during migration. They must be removed by the final cleanup phase.

### Diagnostic Aggregation

Local stages should accumulate diagnostics without owning a `StringTable`.

```rust
pub struct DiagnosticBag {
    diagnostics: Vec<CompilerDiagnostic>,
}
```

Boundary output remains:

```rust
pub struct CompilerMessages {
    pub diagnostics: Vec<CompilerDiagnostic>,
    pub string_table: StringTable,
}
```

`CompilerMessages` should expose helpers:

```rust
impl CompilerMessages {
    pub fn has_errors(&self) -> bool;
    pub fn errors(&self) -> impl Iterator<Item = &CompilerDiagnostic>;
    pub fn warnings(&self) -> impl Iterator<Item = &CompilerDiagnostic>;
}
```

### `CompilerError`

`CompilerError` becomes an internal/tooling/compiler failure type.

It should not be used for malformed Beanstalk source, type errors, borrow errors, import errors, or normal config source diagnostics.

Use one central immediate-print helper:

```rust
pub fn print_compiler_error(error: CompilerError, string_table: Option<&StringTable>);
```

Examples that may remain `CompilerError`:

- file IO failure before source can be represented as diagnostics
- thread panic
- impossible compiler invariant
- backend/tooling infrastructure failure
- dev-server infrastructure failure

The long-term goal is to make most/all `CompilerError` cases impossible, unreachable internal bugs, or removed through better typed flows.

## Proposed Module Layout

```text
src/compiler_frontend/compiler_messages/
  mod.rs

  compiler_diagnostic.rs
  diagnostic_bag.rs
  diagnostic_descriptor.rs
  diagnostic_kind.rs
  diagnostic_payload.rs
  diagnostic_label.rs
  diagnostic_severity.rs

  compiler_error.rs
  source_location.rs

  render/
    mod.rs
    terminal.rs
    terse.rs
    dev_server.rs
    compiler_error_terminal.rs

  tests/
    diagnostic_descriptor_tests.rs
    diagnostic_render_tests.rs
    diagnostic_bag_tests.rs
```

Notes:

- `mod.rs` should be a structural map and public surface.
- Rendering stays owned by `compiler_messages`, but split by output target.
- Avoid a broad `utils` module.
- Move code while improving ownership and names; do not copy the old shape unchanged.

## Phase 0 — Preflight Audit and Dependency Check

### Context

This roadmap must start only after the TypeId redesign is complete enough for diagnostics to refer to canonical frontend type identity.

### Tasks

1. Confirm TypeId completion criteria:
   - common frontend type identity uses compact IDs
   - type names can be rendered from IDs through the type/string tables
   - generic nominal instances have canonical identity
   - old repeated `DataType` payload patterns are not needed for diagnostics

2. Audit current diagnostics:
   - grep for `CompilerError`
   - grep for `CompilerWarning`
   - grep for `return_syntax_error!`
   - grep for `return_type_error!`
   - grep for `return_rule_error!`
   - grep for `return_borrow_checker_error!`
   - grep for `return_compiler_error!`
   - grep for `ErrorMetaDataKey`
   - grep for `ErrorType`
   - grep for `WarningKind`

3. Create a migration matrix listing:
   - call site
   - current error/warning kind
   - stage owner
   - proposed `DiagnosticKind`
   - proposed `DiagnosticPayload`
   - whether the diagnostic can be recovered/accumulated or should stop the stage immediately

4. Identify near-duplicate diagnostics:
   - repeated unknown-name errors
   - repeated type mismatch errors
   - repeated expected-token errors
   - repeated import alias/collision errors
   - repeated borrow conflict errors
   - repeated deferred feature diagnostics

5. Decide which helpers are true owners and which old paths must be deleted later.

### Documentation Updates

None yet, unless this audit discovers stale docs. If stale docs are discovered, record them in the plan notes and update them in the relevant later documentation phase.

### Validation

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo run tests
```

or:

```bash
just validate
```

### Phase Audit Commit

This phase can be one commit containing only audit notes or an initial tracked roadmap task file if desired.

Audit checklist:

- no implementation churn mixed into audit-only work
- no compatibility adapter added yet
- all current diagnostic owners identified
- TypeId dependency confirmed
- duplicated diagnostic construction sites identified

## Phase 1 — Add the New Diagnostic Data Model

### Context

Introduce the new types without changing most call sites. This creates the target architecture and allows focused unit tests before migration begins.

### Tasks

1. Add new files:
   - `compiler_diagnostic.rs`
   - `diagnostic_kind.rs`
   - `diagnostic_payload.rs`
   - `diagnostic_descriptor.rs`
   - `diagnostic_label.rs`
   - `diagnostic_severity.rs`
   - `diagnostic_bag.rs`

2. Define:
   - `CompilerDiagnostic`
   - `DiagnosticSeverity`
   - grouped `DiagnosticKind`
   - grouped subkind enums
   - `DiagnosticPayload`
   - `DiagnosticDescriptor`
   - `DiagnosticLabel`
   - `DiagnosticLabelStyle`
   - `DiagnosticLabelMessage`
   - `DiagnosticBag`

3. Implement:
   - `DiagnosticKind::descriptor()`
   - `DiagnosticKind::category()`
   - `DiagnosticKind::code()`
   - `DiagnosticKind::default_severity()`
   - `CompilerDiagnostic::new(...)`
   - common typed constructors for first migration targets

4. Add initial diagnostic kinds:
   - expected token
   - unexpected token
   - unexpected trailing comma
   - unknown name
   - duplicate declaration
   - type mismatch
   - invalid import
   - deferred feature
   - borrow conflict

5. Add `remap_string_ids` support for:
   - `CompilerDiagnostic`
   - `DiagnosticPayload`
   - `DiagnosticLabel`
   - `DiagnosticBag`
   - eventually `CompilerMessages`

6. Keep the old `CompilerError` and `CompilerWarning` intact for now.

### Documentation Updates

None yet. This phase introduces internal scaffolding only.

### Tests

Add unit tests for:

- descriptor codes are stable and non-empty
- category derives correctly from kind
- severity defaults correctly from descriptor
- explicit severity can override descriptor default
- `DiagnosticBag::has_errors()` works
- string ID remapping touches locations and string payloads

### Validation

Run `just validate`.

### Phase Audit Commit

Audit checklist:

- no rendered user strings inside normal diagnostic constructors
- no generic `Vec<DiagnosticArg>` payload API
- no broad utility module
- new module docs explain ownership
- `mod.rs` remains a structural map
- no stage call sites migrated yet unless trivial and isolated

## Phase 2 — Reshape `CompilerMessages`

### Context

`CompilerMessages` should become the boundary object for ordered diagnostics plus a `StringTable` snapshot.

### Tasks

1. Change `CompilerMessages` to:

```rust
pub struct CompilerMessages {
    pub diagnostics: Vec<CompilerDiagnostic>,
    pub string_table: StringTable,
}
```

2. Add helpers:
   - `empty`
   - `from_diagnostics`
   - `from_bag`
   - `has_errors`
   - `has_warnings`
   - `errors`
   - `warnings`
   - `push`
   - `extend`

3. Temporarily keep conversion helpers:
   - `from_error_compat`
   - `from_warning_compat`
   - `from_legacy_parts_compat`

4. Mark compatibility helpers clearly as temporary migration scaffolding.

5. Update internal callers that only inspect `has_errors`.

6. Update remapping logic to operate on ordered diagnostics.

7. Preserve existing string table boundary behavior.

### Documentation Updates

None yet, unless comments in `compiler_messages` become stale. Update file-level docs in touched modules.

### Tests

Update or add unit tests for:

- ordered diagnostics are preserved
- warning and error iteration works
- `has_errors` ignores warnings
- conversion helpers preserve old diagnostics during migration

### Validation

Run `just validate`.

### Phase Audit Commit

Audit checklist:

- compatibility helpers are narrow and explicitly temporary
- no duplicate permanent message containers exist
- no old `errors`/`warnings` field access remains in touched code
- no stage boundary starts cloning `StringTable` unnecessarily
- warning order is preserved relative to errors where migrated

## Phase 3 — Build the New Render Boundary

### Context

The renderer should be the only normal place where `CompilerDiagnostic` becomes user-visible prose.

### Tasks

1. Create render module:

```text
compiler_messages/render/
  mod.rs
  terminal.rs
  terse.rs
  dev_server.rs
  compiler_error_terminal.rs
```

2. Move path/source-line helpers from `display_messages.rs` into the render module:
   - `resolved_display_path`
   - `resolve_source_file_path`
   - line/column display helpers
   - source snippet loading
   - synthetic `.header` scope handling

3. Implement terminal rendering for `CompilerDiagnostic`:
   - title from descriptor
   - code from descriptor
   - severity from diagnostic
   - category derived from kind
   - message from `kind + payload`
   - primary source location
   - primary/secondary labels
   - help/guidance text from typed payload/label messages

4. Implement terse rendering for `CompilerDiagnostic`.

5. Add temporary legacy rendering adapters:
   - render old `CompilerError` through existing path
   - render old `CompilerWarning` through existing path

6. Add central immediate `CompilerError` printing:
   - `print_compiler_error(error, string_table)`
   - clearly separate from normal diagnostic rendering

7. Keep `display_messages.rs` as a thin compatibility entry point only if needed.

8. Update public rendering calls:
   - `print_compiler_messages`
   - `print_terse_compiler_messages`
   - dev-server error page formatting

### Documentation Updates

None yet, except file/module docs.

### Tests

Update `src/compiler_frontend/compiler_messages/tests/display_messages_tests.rs` or split it into new render tests.

Cover:

- terminal render includes code/title/category
- source location rendering still works
- synthetic header scopes still resolve
- terse format includes stable diagnostic code
- warning diagnostic renders as warning
- immediate `CompilerError` print path is separate

### Validation

Run `just validate`.

### Phase Audit Commit

Audit checklist:

- no frontend stage constructs final diagnostic prose
- render logic owns message wording
- immediate compiler error rendering is centralized
- legacy rendering paths are compatibility-only and named as such
- dev-server output remains functional

## Phase 4 — Convert Warnings into Diagnostics

### Context

Warnings are user-facing diagnostics and should share the same ordered path as errors.

### Tasks

1. Map `WarningKind` variants into grouped `DiagnosticKind` variants.

Recommended initial mapping:

```text
UnusedVariable                         -> Rule(UnusedVariable)
UnusedFunction                         -> Rule(UnusedFunction)
UnusedImport                           -> Import(UnusedImport)
UnusedType                             -> Rule(UnusedType)
UnusedConstant                         -> Rule(UnusedConstant)
UnusedFunctionArgument                 -> Rule(UnusedFunctionArgument)
UnusedFunctionReturnValue              -> Rule(UnusedFunctionReturnValue)
UnusedFunctionParameter                -> Rule(UnusedFunctionParameter)
UnusedFunctionParameterDefaultValue    -> Rule(UnusedFunctionParameterDefaultValue)
PointlessExport                        -> Import(PointlessExport)
MalformedCssTemplate                   -> Syntax(MalformedCssTemplate)
MalformedHtmlTemplate                  -> Syntax(MalformedHtmlTemplate)
BstFilePathInTemplateOutput            -> Rule(BstFilePathInTemplateOutput)
LargeTrackedAsset                      -> Rule(LargeTrackedAsset)
IdentifierNamingConvention             -> Rule(IdentifierNamingConvention)
ImportAliasCaseMismatch                -> Import(ImportAliasCaseMismatch)
UnreachableMatchArm                    -> Rule(UnreachableMatchArm)
```

2. Add warning constructors:

```rust
CompilerDiagnostic::unused_variable(...)
CompilerDiagnostic::import_alias_case_mismatch(...)
CompilerDiagnostic::unreachable_match_arm(...)
```

3. Replace warning emission sites to push `CompilerDiagnostic` with severity `Warning`.

4. Remove or shrink `CompilerWarning`.

5. Remove warning-specific rendering branches once migrated.

6. Update tests and integration assertion helpers.

### Documentation Updates

None yet, except module docs and comments.

### Tests

Cover:

- warnings remain non-fatal
- warnings preserve emission order
- warning diagnostics render correctly
- warning-only builds still succeed
- integration tests expecting warnings still work

### Validation

Run `just validate`.

### Phase Audit Commit

Audit checklist:

- `CompilerWarning` removed or reduced to a temporary adapter
- no separate warning rendering path remains for migrated warnings
- warning diagnostics have stable codes
- no warning stores final prose in payload
- all warning tests assert codes where practical

## Phase 5 — Convert Syntax and Tokenizer/Header Diagnostics

### Context

Syntax and structural header diagnostics are good early conversion targets because many have compact payloads.

### Tasks

1. Convert tokenizer diagnostics:
   - unknown/invalid token
   - unterminated string/template
   - invalid character literal
   - unknown style directive syntax where owned by tokenizer

2. Convert parser/header diagnostics:
   - expected token
   - unexpected token
   - unexpected trailing comma
   - invalid top-level syntax
   - malformed declaration shell
   - invalid import syntax
   - invalid grouped import aliasing
   - invalid `#mod.bst` facade syntax
   - invalid top-level runtime statement in non-entry file

3. Replace `return_syntax_error!` call sites in converted areas with typed constructors.

4. Use `DiagnosticBag` where the stage can collect multiple errors.

5. Stop at safe boundaries when syntax errors exist.

6. Keep stage ownership clear:
   - tokenizer rejects lexical syntax
   - headers reject top-level/import/declaration shell syntax
   - AST does not rediscover top-level syntax

### Documentation Updates

If the changed syntax diagnostics expose stale docs, update only the relevant concise section in:

- `docs/compiler-design-overview.md`
- `docs/codebase-style-guide.md`

Otherwise defer docs updates to the final documentation phase.

### Tests

Add/update integration tests for:

- trailing comma rejection in function return signatures
- grouped import alias errors
- malformed imports
- unknown directives
- unterminated templates
- top-level runtime statements in non-entry files

Assertions should prefer diagnostic codes over fragile prose fragments.

### Validation

Run `just validate`.

### Phase Audit Commit

Audit checklist:

- no converted syntax diagnostics use `CompilerError`
- no converted syntax diagnostics store final prose
- syntax recovery does not blur tokenizer/header/AST boundaries
- repeated expected-token logic is centralized
- tests assert stable codes

## Phase 6 — Convert Import, Name Resolution, and Rule Diagnostics

### Context

Import/name diagnostics are currently likely to contain duplicate wording and string metadata. This phase should consolidate those diagnostics into typed kinds and payloads.

### Tasks

1. Convert import diagnostics:
   - missing import target
   - private module/file boundary violation
   - bare file import rejected
   - alias collision
   - alias case mismatch warning
   - grouped import errors
   - unsupported external package import
   - module facade visibility violation

2. Convert name resolution diagnostics:
   - unknown value name
   - unknown type name
   - value/type namespace misuse
   - duplicate declaration
   - shadowing/redeclaration
   - reserved builtin/prelude name collision
   - invalid `this` usage

3. Convert semantic rule diagnostics:
   - invalid top-level constant usage
   - non-constant reference in constant
   - invalid assignment target
   - invalid mutable access syntax
   - invalid receiver method call shape
   - invalid multi-bind usage
   - invalid match arm shape where AST-owned

4. Replace `return_rule_error!` call sites in converted areas.

5. Prefer typed payloads:
   - `StringId` for names
   - declaration/source IDs where available
   - `SourceLocation` for secondary previous declarations
   - namespace enums rather than strings

6. Deduplicate repeated diagnostics into shared constructors owned by the relevant subsystem or by `compiler_messages` when genuinely shared.

### Documentation Updates

If import/name/visibility behavior is clarified or changed, update concise text in:

- `docs/language-overview.md`
- `docs/compiler-design-overview.md`
- `docs/src/docs/progress/#page.bst`

### Tests

Add/update integration tests for:

- duplicate declarations
- import alias collisions
- case mismatch warnings
- value/type namespace misuse
- private module visibility
- unknown names
- invalid `this`

Assertions should include diagnostic code and key rendered payload where useful.

### Validation

Run `just validate`.

### Phase Audit Commit

Audit checklist:

- import visibility logic is not duplicated in AST
- diagnostic constructors are not scattered across stages when semantically identical
- no stringly typed metadata remains in converted diagnostics
- previous declaration labels are used where useful
- no compatibility wrappers become permanent

## Phase 7 — Convert Type Diagnostics

### Context

This phase depends directly on TypeId. Type diagnostics should carry type identity and context, not formatted strings or cloned `DataType` payloads.

### Tasks

1. Convert common type diagnostics:
   - type mismatch
   - invalid coercion
   - unsupported operator types
   - invalid function argument type
   - invalid return type
   - invalid declaration annotation
   - invalid struct field type
   - invalid choice payload type
   - invalid option/result use
   - invalid collection element type
   - invalid empty collection inference

2. Add typed contexts:

```rust
pub enum TypeMismatchContext {
    Assignment,
    Return,
    FunctionArgument,
    ConstructorArgument,
    StructFieldDefault,
    CollectionElement,
    TemplateInterpolation,
    MatchScrutinee,
    MatchArm,
}
```

3. Add renderer support to resolve:
   - `TypeId -> source-level type name`
   - aliases where useful
   - generic nominal instances
   - builtin option/result formatting

4. Replace `return_type_error!` call sites in converted areas.

5. Ensure type rendering uses the shared type table/string table and does not require carrying `DataType` strings in the diagnostic.

6. Consolidate type compatibility/coercion diagnostic construction near `type_coercion` or the boundary owner that applies contextual coercion.

### Documentation Updates

Update concise relevant sections in:

- `docs/compiler-design-overview.md`
  - explain that type diagnostics carry `TypeId` and render at boundary
- `docs/codebase-style-guide.md`
  - require TypeId payloads for type diagnostics

Update `docs/src/docs/progress/#page.bst` if TypeId-backed diagnostics are marked as implemented or partial.

### Tests

Add/update tests for:

- assignment type mismatch
- return type mismatch
- function argument mismatch
- collection element mismatch
- empty collection ambiguity
- operator type mismatch
- option/result misuse
- generic nominal instance mismatch if generics are implemented enough

Assertions should use diagnostic codes and key rendered type names.

### Validation

Run `just validate`.

### Phase Audit Commit

Audit checklist:

- no type diagnostic stores rendered type strings
- no type diagnostic clones large `DataType` payloads for display
- type rendering path is centralized
- contextual coercion owners produce diagnostics consistently
- aliases and generic names render clearly

## Phase 8 — Convert Borrow Checker Diagnostics

### Context

Borrow diagnostics need secondary labels and structured payloads to become readable and machine-useful.

### Tasks

1. Convert borrow checker diagnostics:
   - multiple mutable borrows
   - shared/mutable conflict
   - use after move
   - move while borrowed
   - whole-object borrow conflict
   - invalid mutable access
   - invalid access after possible ownership transfer

2. Replace borrow checker diagnostic macros:
   - `create_multiple_mutable_borrows_error!`
   - `return_multiple_mutable_borrows_error!`
   - `create_shared_mutable_conflict_error!`
   - similar borrow macros

3. Add typed payloads:
   - place identity or rendered place payload
   - existing access kind
   - requested access kind
   - move/borrow source locations
   - ownership effect where useful

4. Use labels:
   - primary invalid use
   - secondary existing borrow/move
   - secondary previous access where useful

5. Ensure borrow validation still produces side-table facts and does not mutate HIR.

### Documentation Updates

Update concise relevant sections in:

- `docs/memory-management-design.md`
  - only if diagnostic wording or reporting responsibilities need clarification
- `docs/compiler-design-overview.md`
  - mention borrow validation emits `CompilerDiagnostic` and side-table facts

### Tests

Add/update borrow checker tests for:

- multiple mutable borrows
- shared then mutable conflict
- mutable then shared conflict
- use after possible move
- move while borrowed
- whole-object borrow restrictions

Assertions should prefer codes and label-sensitive rendered fragments.

### Validation

Run `just validate`.

### Phase Audit Commit

Audit checklist:

- no borrow diagnostics use string metadata maps
- secondary locations are preserved
- no borrow checker facts are mixed into diagnostic-only structures
- no HIR mutation is introduced
- old borrow macros are removed if fully migrated

## Phase 9 — Convert Deferred Feature and Backend/Builder-Facing Diagnostics

### Context

Deferred features should become explicit diagnostic kinds. Backend/build-system errors must be separated between user-facing config/source diagnostics and internal/tooling failures.

### Tasks

1. Convert `deferred_feature_diagnostics.rs` into explicit `DiagnosticKind::DeferredFeature(...)` constructors.

2. Add stable codes for deferred/reserved syntax:
   - reserved but unimplemented syntax
   - deferred nested patterns
   - deferred package features
   - unsupported backend-specific frontend-visible features

3. Audit backend builder validation:
   - config source errors should become `CompilerDiagnostic`
   - backend/tooling failures should remain `CompilerError` and print immediately
   - source-library/external-package user errors should become diagnostics

4. Audit project config loading:
   - parsed `#config.bst` semantic errors should be diagnostics
   - raw file load/IO failures may remain `CompilerError`

5. Update dev-server error page to display ordered diagnostics.

### Documentation Updates

Update concise relevant sections in:

- `docs/compiler-design-overview.md`
- `docs/src/docs/progress/#page.bst`

Add deferred progress entries for:
- full JSON diagnostic CLI output
- full LSP diagnostic protocol mapping
- docs-linked expanded diagnostics
- localization

### Tests

Add/update tests for:

- deferred feature diagnostics
- config source diagnostics
- unsupported external package import
- dev-server diagnostic display shape if covered by tests

### Validation

Run `just validate`.

### Phase Audit Commit

Audit checklist:

- deferred features are not emitted as ad-hoc strings
- backend/user-source diagnostic boundary is clear
- infrastructure failures are not disguised as user diagnostics
- dev-server preserves diagnostic order and code display
- deferred roadmap/matrix entries are updated

## Phase 10 — Remove Legacy `CompilerError` User-Diagnostic Path

### Context

Once major diagnostic families are migrated, remove compatibility layers and make the new model the only normal user-diagnostic path.

### Tasks

1. Delete or rewrite:
   - `return_syntax_error!`
   - `return_type_error!`
   - `return_rule_error!`
   - `return_borrow_checker_error!`
   - old borrow diagnostic macros
   - old `ErrorMetaDataKey`
   - old user-facing `ErrorType` variants
   - old `CompilerWarning`
   - warning-specific rendering branches
   - legacy `CompilerMessages` conversion helpers

2. Shrink `CompilerError` to internal/tooling failures only.

3. Add compiler error immediate-print path at all remaining boundaries.

4. Update all `Result<T, CompilerError>` signatures that now represent user-source diagnostics to use:
   - `Result<T, DiagnosticBag>`
   - `Result<T, CompilerMessages>`
   - or a stage-specific result alias

5. Keep internal invariant errors separate and explicit.

6. Search for:
   - `CompilerError::new_syntax_error`
   - `CompilerError::new_rule_error`
   - `CompilerError::new_type_error`
   - `CompilerError::new_borrow_checker_error`
   - `ErrorType::Syntax`
   - `ErrorType::Type`
   - `ErrorType::Rule`
   - `ErrorType::BorrowChecker`
   - `metadata`
   - old macros

7. Delete stale comments and examples describing the old model.

### Documentation Updates

Update:

- `docs/compiler-design-overview.md`
- `docs/codebase-style-guide.md`
- `docs/src/docs/progress/#page.bst`
- `docs/roadmap/roadmap.md` only if its concise milestone text is stale

Do not add a step to manually link this plan from the roadmap; that is intentionally handled outside this plan.

### Tests

Update:

- compiler message unit tests
- integration test runner assertions
- dev-server tests
- CLI/check command tests if present
- any tests matching old terse format

All diagnostics should now assert stable codes where practical.

### Validation

Run `just validate`.

### Phase Audit Commit

Audit checklist:

- no normal user-source diagnostic is represented as `CompilerError`
- no old macro remains for user diagnostics
- no string metadata map remains
- no separate `CompilerWarning` path remains
- `CompilerError` cannot silently enter `CompilerMessages`
- docs no longer instruct users/agents to create string-backed errors
- test fixtures no longer depend on fragile old prose except where rendering text is contractual

## Phase 11 — Final Documentation and Roadmap Cleanup

### Context

The implementation should end with docs that describe the current system, not the migration.

### Tasks

1. Update `docs/compiler-design-overview.md` concisely:
   - `compiler_messages/` owns typed diagnostics, source locations, render-boundary aggregation, renderers, and immediate compiler error printing.
   - `CompilerDiagnostic` is for user-facing source/config/language diagnostics.
   - `CompilerError` is for internal/tooling/compiler failures only.
   - categories derive from `DiagnosticKind`.
   - type diagnostics use `TypeId` payloads.
   - renderers resolve names/paths/types at the boundary.

2. Update `docs/codebase-style-guide.md` concisely:
   - user-source diagnostics must use typed constructors.
   - do not create ad-hoc rendered strings in frontend stages.
   - do not use `CompilerError` for user source mistakes.
   - use `DiagnosticBag` for local accumulation.
   - use central immediate-print helper for `CompilerError`.
   - prefer diagnostic codes in tests.

3. Update `docs/src/docs/progress/#page.bst`:
   - mark structured diagnostic redesign as implemented or partial according to actual state.
   - add deferred entries for:
     - JSON diagnostic CLI output
     - LSP diagnostic protocol mapping
     - docs-linked expanded diagnostics
     - localization
     - diagnostic-code docs generation

4. Update `docs/roadmap/roadmap.md` only if concise milestone/status text needs to reflect:
   - typed diagnostics complete
   - full JSON/LSP renderer deferred

5. Keep all documentation concise. Do not add large explanatory sections unless needed.

### Tests

No new runtime tests required unless docs build checks depend on generated docs artifacts.

### Validation

Run `just validate`.

### Phase Audit Commit

Audit checklist:

- docs match implementation
- no migration-only text remains as permanent design
- progress matrix includes deferred JSON/LSP work
- roadmap is concise
- no docs claim full JSON/LSP support exists before implementation

## Final Acceptance Criteria

This roadmap item is complete when:

- `CompilerMessages` stores one ordered `Vec<CompilerDiagnostic>`.
- `CompilerWarning` is removed or fully migrated into `CompilerDiagnostic`.
- User-source diagnostics no longer use `CompilerError`.
- `DiagnosticKind` is grouped and maps to stable explicit diagnostic codes.
- Diagnostic categories are derived from `DiagnosticKind`.
- User diagnostics render from `DiagnosticKind + DiagnosticPayload`.
- Type diagnostics carry `TypeId` and render type names only at the boundary.
- Existing tests pass with updated diagnostic assertions.
- High-frequency syntax/type/rule/import/borrow diagnostics are typed.
- `CompilerError` is only used for immediate compiler/tooling/internal failure printing.
- The old user-facing `return_*_error!` macros are deleted.
- The old string metadata map is deleted.
- Docs and progress/roadmap entries are updated.
- Full JSON/LSP output is explicitly deferred in the roadmap/progress matrix.

## Deferred Follow-Up Work

Track these separately after this roadmap item:

- Full JSON diagnostic CLI output.
- Full LSP diagnostic protocol mapping.
- Localization.
- Docs-linked expanded explanations for each diagnostic code.
- Automatic diagnostic-code documentation generation.
- More sophisticated parser recovery.
- Diagnostic snapshots/goldens for terminal rendering if the terminal output becomes contractual.
- IDE quick-fix payloads using typed suggestions.
