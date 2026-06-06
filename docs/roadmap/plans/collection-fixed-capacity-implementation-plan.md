# Collection Fixed-Capacity Type Extension Implementation Plan

Target feature: fixed collection type constraints using `{N T}` and folded constant capacity expressions.

Repository target: `nyejames/beanstalk` on `main`.

## 1. Goal

Implement fixed collections as a real collection type shape.

```beanstalk
items {64 Int} = {1, 2, 3}
mutable_items ~{64 Int} = {}
capacity #Int = 48
names ~{capacity + 16 String} = {}
```

This feature is **not** a growable collection preallocation hint. It adds fixed collections with a compile-time-known maximum length.

| Syntax | Meaning |
|---|---|
| `{T}` | Growable collection of `T`. |
| `{N T}` | Fixed collection of `T` with exact maximum length `N`. |
| `~{T}` | Mutable access or binding to a growable collection of `T`. |
| `~{N T}` | Mutable access or binding to a fixed collection of `T` with exact maximum length `N`. |

`~` remains access and binding mode. It is not part of collection type identity.

## 2. Current Repository Anchors

The current repo already contains an older partial capacity path. Replace it instead of extending it.

Key anchors:

- `src/compiler_frontend/declaration_syntax/type_syntax/parse.rs`
  - currently defines `CollectionCapacity { value: i64, location }`;
  - currently parses capacity after the element type;
  - currently stores capacity outside the parsed type in `ParsedTypeAnnotation.collection_capacity`.
- `src/compiler_frontend/declaration_syntax/declaration_shell.rs`
  - `DeclarationSyntax` and `BindingTargetSyntax` carry `collection_capacity` side fields marked as not wired to codegen.
- `src/compiler_frontend/datatypes/parsed.rs`
  - `ParsedTypeRef::Collection` currently stores only `element` and `location`.
- `src/compiler_frontend/datatypes/ids.rs`
  - `BuiltinTypeConstructor::Collection` is currently payload-free;
  - `ConstructedTypeKey` currently keys constructed types by constructor plus type arguments.
- `src/compiler_frontend/datatypes/environment.rs`
  - `TypeEnvironment::intern_constructed` interns collection types only by constructor and element argument.
- `src/compiler_frontend/datatypes/generic_identity_bridge.rs`
  - `TypeIdentityKey::Collection` currently stores only the element identity key.
- `src/compiler_frontend/ast/type_resolution/resolve_type.rs`
  - `resolve_parsed_type_annotation` currently converts `ParsedTypeRef` to diagnostic `DataType`, resolves it, then converts to `TypeId`.
  - Fixed capacity folding should not be hidden inside diagnostic `DataType` conversion.
- `src/compiler_frontend/ast/statements/collections.rs`
  - collection literal parsing currently receives only `Option<TypeId>` for the element type.
- `src/compiler_frontend/ast/field_access/collection_builtin.rs`
  - mutable receiver access is already required for `set`, `push`, and `remove`;
  - `get`, `set`, and `remove` are fallible;
  - `push` is currently infallible.
- `src/libraries/core/collections.rs`
  - core collection builtin metadata exists for `get`, `set`, `push`, `remove`, and `length`.
- `src/compiler_frontend/hir/expressions.rs`
  - `HirExpressionKind::Collection(Vec<HirExpression>)` carries no capacity field; it relies on the expression `ty`.
- `src/backends/js/js_expr.rs`
  - collection expressions currently lower to plain JavaScript array literals.
- `src/backends/js/runtime/collections.rs`
  - collection helpers currently assume arrays;
  - runtime comments say `push` and `length` are infallible.
- `src/projects/html_project/js_path.rs`
  - the HTML JS path already passes the module `TypeEnvironment` into JS lowering.

## 3. Design Contract

### 3.1 User-Facing Terminology

Use:

- **collection** for growable `{T}`;
- **fixed collection** for `{N T}`.

Do not describe this feature as “immutable collections” versus “mutable collections.” Mutability is access permission. Fixedness is a collection type shape.

### 3.2 Semantic Type Shape

A collection type has:

```rust
pub(crate) struct CollectionShape {
    pub(crate) element_type: TypeId,
    pub(crate) fixed_capacity: Option<usize>,
}
```

Rules:

- `fixed_capacity: None` means growable collection.
- `fixed_capacity: Some(n)` means fixed collection with exact maximum length `n`.
- `n == 0` is invalid in authored fixed collection types.
- `{Int}` and `{64 Int}` are different semantic collection types.
- `{64 Int}` and `{128 Int}` are incompatible.
- `{64 Int}` and `{Int}` are incompatible.
- There is no implicit fixed/growable conversion.
- There is no “at least N” capacity constraint in this feature.
- Type aliases preserve fixed collection shape.
- Generic inference treats fixed and growable collection shapes as distinct concrete type arguments.
- `~` stays in binding/access metadata and borrow validation. It must not be stored in the collection type shape.

### 3.3 Capacity Expressions

Capacity accepts compile-time `Int` expressions.

```beanstalk
capacity #Int = 64
items ~{capacity Int} = {}
items ~{capacity + 16 Int} = {}
```

Rules:

- Capacity expressions are parsed in type position.
- They fold during existing AST declaration/type resolution.
- Do not add an AST stage.
- Type annotations must resolve before the RHS is parsed/type-checked.
- Top-level capacity expressions create dependency edges to referenced constants.
- Body-local capacity expressions may reference constants already declared and visible before the type use.
- Capacity constants use ordinary Beanstalk value naming, such as `capacity`; all-caps names are reserved for traits.
- The folded value must be a positive `Int` and fit in `usize`.
- Runtime calls, runtime variables, templates, collection literals, floats, non-`Int` values, non-foldable expressions, negative values, and zero are invalid.

### 3.4 Parsing Boundary

Inside collection type braces:

- `{T}` parses as a growable collection type.
- `{N T}` parses as a fixed collection type.
- The first valid element type annotation ends the capacity expression.
- Tokens after the element type before `}` are invalid.

Examples:

```beanstalk
items {Int} = {}
items {capacity + 16 Int} = {}
boxes {capacity Box of String} = {}
maybe_items {capacity Int?} = {}
grid {rows {cols Int}} = {{1}, {2}}
```

The parser must not rely on a lossy post-element capacity side-channel. It should represent capacity as part of the parsed collection type.

### 3.5 Capacity-Only Shorthand

Capacity-only shorthand is valid only for binding declarations with an immediate non-empty collection literal initializer that can infer the element type.

```beanstalk
items {48} = {"0", "2"}
items ~{capacity + 16} = {"0"}
```

Invalid:

```beanstalk
items {48} = {}        -- ambiguous element type
items {48} = make()    -- no immediate literal element evidence
take |items {48}|:     -- public/API surface needs an element type
;
Names as {48}          -- aliases need an element type
Field = | values {48} | -- fields need an element type
```

### 3.6 Collection Literals

Collection literals are context-typed.

```beanstalk
items = {1, 2, 3}       -- growable {Int}
items ~= {1, 2, 3}      -- mutable binding to growable {Int}
fixed {4 Int} = {1, 2}  -- fixed {4 Int}
```

Rules:

- Unannotated literals infer growable `{T}`.
- In a fixed receiving context, a literal constructs a fixed collection directly.
- Fixed literal construction is not implicit growable-to-fixed conversion.
- Empty collection literals still require an element type context.
- Fixed collection literal length must be `<= fixed_capacity`.

### 3.7 Empty Fixed Collections

Reject statically empty immutable fixed collection bindings.

```beanstalk
items {64 Int} = {} -- invalid
```

Valid:

```beanstalk
items ~{64 Int} = {}
items {64 Int} = {1, 2}
items {64 Int} = make_items()
```

Rules:

- Reject only immutable value bindings initialized with a statically empty fixed collection literal.
- Allow mutable fixed empty bindings.
- Allow immutable fixed declarations from non-empty literals.
- Allow immutable fixed declarations from calls/expressions returning `{N T}` because current length is runtime state.
- Do not add length refinement tracking.
- Fixed collection fields may have empty defaults when later mutation goes through a mutable owner path.

```beanstalk
Buffer = |
    items {64 Int} = {},
|

buffer ~= Buffer()
~buffer.items.push(1)!
```

### 3.8 Collection Methods

| Receiver access | `length` | `get` | `push` | `set` | `remove` |
|---|---:|---:|---:|---:|---:|
| shared `{T}` / `{N T}` | yes | yes | no | no | no |
| mutable `~{T}` / `~{N T}` | yes | yes | yes | yes | yes |

Rules:

- Mutating methods require explicit `~receiver`.
- `get` on a shared receiver gives shared element access.
- `get` on a mutable receiver may give mutable element access when borrow validation allows it.
- No assignment-through-`.get()` is introduced.
- `set(index, value)` replaces an existing element only. It does not fill unused fixed capacity.
- `remove(index)` returns `Elem, Error!`, shifts later elements down, and preserves fixed capacity.
- `push(value)` has no success value but is fallible for all collections.
- `length()` remains infallible.

`push` fallibility:

- Fixed collections fail when current length equals fixed capacity.
- Growable collections may fail for allocation/backend/runtime limits.
- Source must handle `push` with `!` or `catch`.

```beanstalk
items ~{2 Int} = {}
~items.push(1)!
~items.push(2)!
~items.push(3) catch |err|:
    io(err.message)
;
```

### 3.9 JavaScript Runtime Semantics

Fixed capacity is language semantics and must be enforced by JS.

Rules:

- Growable `{T}` may continue to lower to a JavaScript array.
- Fixed `{N T}` must lower to a wrapper/helper representation that carries fixed capacity.
- Runtime helpers must operate on both growable arrays and fixed wrappers.
- Fixed `push` checks `length < fixed_capacity`.
- `get`, `set`, and `remove` use dense logical length.
- `length` returns logical length, not fixed capacity.
- Keep JS simple; serious fixed-layout optimization belongs to future Wasm/backend work.

Recommended JS representation:

```js
{
  __bst_kind: "fixed_collection",
  items: [],
  fixedCapacity: 64,
}
```

### 3.10 Deferred Features

Do not implement these in this plan:

- Growable initial capacity hints.
- Fixed/growable explicit conversion through `copy`.
- General `CAST` / conversion traits.
- Capacity subtyping or “at least N” constraints.
- Runtime capacity expressions.
- Default-fill syntax:

  ```beanstalk
  slots {64 Item?} = {...none}
  zeros {1024 Int} = {...0}
  defaults {42 SomeStruct} = {...SomeStruct}
  ```

- Struct/choice default-value validation for default-fill syntax.
- Wasm fixed storage optimization beyond preserving type facts.

Roadmap notes must explicitly mention the deferred default-fill exploration, growable capacity hints, and explicit conversion after cast/copy hardening.

## 4. Implementation Principles

- Keep one semantic owner: `TypeEnvironment`.
- Keep one syntax owner: `declaration_syntax/type_syntax`.
- Keep capacity folding in AST type/declaration resolution.
- Keep top-level ordering in headers/dependency sorting.
- Do not add HIR side tables for capacity.
- Do not add compatibility wrappers for old capacity syntax.
- Do not preserve the old post-element capacity parser path.
- Avoid broad subtyping. Exact `TypeId` shape compatibility is enough.
- Reuse existing constant folding and typed diagnostics.
- Prefer small helper structs over long parameter lists.
- Prefer one collection shape query API over repeated ad hoc matches.

Recommended collection query API:

```rust
pub(crate) struct CollectionShape {
    pub(crate) element_type: TypeId,
    pub(crate) fixed_capacity: Option<usize>,
}

impl TypeEnvironment {
    pub(crate) fn collection_shape(&self, type_id: TypeId) -> Option<CollectionShape>;
    pub(crate) fn collection_element_type(&self, type_id: TypeId) -> Option<TypeId>;
    pub(crate) fn collection_fixed_capacity(&self, type_id: TypeId) -> Option<usize>;
}
```

`collection_shape` distinguishes non-collections from growable collections. Avoid `Option<Option<usize>>` at call sites.

## 5. Implementation Phases

Each phase ends with:

1. **Audit** — inspect touched files for stale paths, duplicate logic, and stage-boundary leaks.
2. **Style guide review** — verify typed diagnostics, clear ownership, no user-input panics, no compatibility shims, and no dense clever code.
3. **Validation** — run targeted tests, then full validation when the phase is stable.

Use `just validate` as the final full validation command.

---

## Phase 0 — Repo Sync and Baseline Audit

### Context

The repo contains a partial capacity side-channel. Confirm every old path before deleting it.

### Tasks

- [ ] Confirm a clean working tree.

  ```bash
  git status --short
  git rev-parse HEAD
  ```

- [ ] Run focused searches.

  ```bash
  rg "CollectionCapacity|collection_capacity|NegativeCollectionCapacity|InvalidCollectionTypeReason"
  rg "ParsedTypeRef::Collection|parse_collection_type|parsed_ref_to_data_type"
  rg "intern_collection|collection_element_type|BuiltinTypeConstructor::Collection|TypeIdentityKey::Collection"
  rg "CollectionBuiltinOp|__bs_collection_push|__bs_collection_get|__bs_collection_remove"
  rg "ExpressionKind::Collection|HirExpressionKind::Collection"
  ```

- [ ] List every fixture currently using `~items.push(...)` without `!` or `catch`.
- [ ] List parser/unit tests assuming old capacity placement.
- [ ] Record touched files in implementation notes.

### Audit / Style / Validation

- [ ] Confirm old capacity metadata has no compatibility requirement.
- [ ] Confirm the intended final representation is parsed type shape plus canonical `TypeId` shape.
- [ ] Run:

  ```bash
  cargo check
  ```

---

## Phase 1 — Replace Parsed Capacity Side-Channel

### Context

Capacity belongs in `ParsedTypeRef::Collection`, not in declaration shells. This deletes redundant state and prevents future code from threading two capacity representations.

### Tasks

- [ ] Remove `CollectionCapacity` and `ParsedTypeAnnotation.collection_capacity` from `src/compiler_frontend/declaration_syntax/type_syntax/parse.rs`.
- [ ] Remove `collection_capacity` from `DeclarationSyntax` and `BindingTargetSyntax` in `src/compiler_frontend/declaration_syntax/declaration_shell.rs`.
- [ ] Add parsed capacity expression data to `src/compiler_frontend/datatypes/parsed.rs`.

  Recommended shape:

  ```rust
  pub(crate) struct ParsedCollectionCapacity {
      pub(crate) tokens: Vec<Token>,
      pub(crate) location: SourceLocation,
  }

  ParsedTypeRef::Collection {
      element: Box<ParsedTypeRef>,
      fixed_capacity: Option<ParsedCollectionCapacity>,
      location: SourceLocation,
  }
  ```

- [ ] Use `ParsedTypeRef::Inferred` as the element only for capacity-only shorthand in declaration target context.
- [ ] Add token remapping for capacity tokens through `ParsedTypeRef::remap_string_ids`.
- [ ] Update `parsed_ref_to_data_type` to preserve fixed collection display spelling through diagnostic `DataType`.
  - Preferred low-churn path: extend `BuiltinGenericType::Collection` with `fixed_capacity: Option<usize>` only after folding; before folding, diagnostic spelling can remain source-ref based where needed.
  - Do not use diagnostic `DataType` to make semantic capacity decisions.

### Parser Strategy

- [ ] Introduce a small type-syntax parse context if needed instead of adding more loose parameters.

  Recommended shape:

  ```rust
  struct TypeSyntaxParseContext<'a> {
      annotation_context: TypeAnnotationContext,
      string_table: &'a StringTable,
  }
  ```

- [ ] Parse collection brace contents in a controlled helper rather than mutating the main token stream speculatively.
- [ ] Classify type-start tokens structurally:
  - builtin datatype tokens;
  - `{` for nested collection types;
  - `TraitThis` only in trait requirement context;
  - type-style symbol names such as `Box`, `Person`, or generic parameters;
  - namespace-qualified type syntax such as `canvas.Canvas2d`.
- [ ] Treat lower-snake bare symbols as capacity-expression candidates, not type starts.
- [ ] For `namespace.TypeName`, use the member spelling to recognize type syntax; lower-case member access such as `settings.capacity` remains capacity-expression syntax.
- [ ] Implement the agreed boundary rule:
  - [ ] if content starts with a valid element type and the type consumes all content, parse growable `{T}`;
  - [ ] otherwise, scan for the first valid element type suffix;
  - [ ] tokens before that suffix become the capacity expression;
  - [ ] the suffix becomes the element type;
  - [ ] tokens after the suffix are an error;
  - [ ] if no element type exists and the context is a declaration target, parse capacity-only shorthand;
  - [ ] if no element type exists in signatures/aliases/fields/returns, report shorthand-not-allowed.

### Audit / Style / Validation

- [ ] Confirm `DeclarationSyntax` and `BindingTargetSyntax` no longer carry capacity.
- [ ] Confirm old `{Int 64}` syntax is rejected or not parsed as a capacity form.
- [ ] Confirm parser code contains no semantic folding or type environment access.
- [ ] Run:

  ```bash
  cargo test declaration_syntax
  cargo check
  ```

---

## Phase 2 — Canonical TypeEnvironment Collection Shape

### Context

Fixed capacity is semantic type identity. Add it to canonical type interning before AST tries to resolve fixed collection annotations.

### Tasks

- [ ] Update `src/compiler_frontend/datatypes/ids.rs`.

  Preferred low-noise shape:

  ```rust
  pub enum BuiltinTypeConstructor {
      Collection { fixed_capacity: Option<usize> },
      Option,
      FallibleCarrier,
      Tuple,
  }
  ```

  This keeps fixed capacity inside the existing constructed-type key instead of adding a parallel metadata table.

- [ ] Add or update `TypeEnvironment::intern_collection`.

  ```rust
  pub(crate) fn intern_collection(
      &mut self,
      element_type: TypeId,
      fixed_capacity: Option<usize>,
  ) -> TypeId
  ```

- [ ] Route all collection interning through `intern_collection`.
- [ ] Keep `intern_constructed` general, but stop direct collection callers from manually building collection constructed keys.
- [ ] Add `CollectionShape` query helpers to `TypeEnvironment`.
- [ ] Update `display_type` / constructed display to render:
  - `{Int}`;
  - `{64 Int}`;
  - nested forms such as `{4 {8 Int}}`.
- [ ] Update `generic_identity_bridge.rs`.

  Recommended shape:

  ```rust
  TypeIdentityKey::Collection {
      element: Box<TypeIdentityKey>,
      fixed_capacity: Option<usize>,
  }
  ```

- [ ] Update diagnostic bridge conversion in both directions so fixed collection shape survives:
  - [ ] generic nominal instantiation;
  - [ ] generic function inference;
  - [ ] HIR generic identity registration;
  - [ ] diagnostic rendering.
- [ ] Update substitution paths; constructed type substitution should preserve `BuiltinTypeConstructor::Collection { fixed_capacity }` while substituting only the element argument.
- [ ] Update type compatibility only where needed.
  - Exact `TypeId` equality should reject fixed/growable mismatch and exact-capacity mismatch.
  - Do not add an element-only compatibility fallback except in literal context typing.

### Audit / Style / Validation

- [ ] Confirm `{64 Int}` and `{Int}` intern to different `TypeId`s.
- [ ] Confirm `{capacity Int}` and `{64 Int}` intern to the same `TypeId` when `capacity #Int = 64`.
- [ ] Confirm aliases preserve fixed collection shape.
- [ ] Confirm `~` is absent from type identity.
- [ ] Run:

  ```bash
  cargo test datatypes
  cargo test type_coercion
  cargo check
  ```

---

## Phase 3 — Capacity Dependencies and AST Folding

### Context

Capacity expressions are compile-time value expressions in type position. They need dependency edges at the header stage and folding at AST type resolution.

### Tasks

- [ ] Extract shared symbol-reference scanning for token slices.
  - Prefer moving the useful parts of `collect_initializer_references` into a small shared helper under `declaration_syntax` or `token_scan`.
  - Use it for both initializer references and capacity-expression references.
  - Avoid duplicating shallow `symbol`, `symbol.member`, call, and namespace filtering logic.
- [ ] Extend parsed type walking to expose capacity expression reference hints.
- [ ] Add top-level dependency edges for capacity expressions in:
  - [ ] declaration annotations;
  - [ ] type aliases;
  - [ ] struct fields and defaults where type annotations contain capacity;
  - [ ] choice payload fields;
  - [ ] function parameters;
  - [ ] function returns;
  - [ ] facade/public declarations.
- [ ] Preserve existing constant ordering semantics.
  - Same-file forward constant use in capacity position follows the same rules as other constant uses.
  - Cross-file capacity constants use existing dependency sorting.
- [ ] Update `resolve_parsed_type_annotation` so collection parsed refs resolve through a parsed-ref-first path.
  - Do not rely on `parsed_ref_to_data_type` to produce semantic fixed capacity.
  - Keep `DataType` as diagnostic spelling only.
- [ ] Add a focused capacity-folding helper in AST type resolution.

  Recommended shape:

  ```rust
  fn fold_collection_capacity_expression(
      capacity: &ParsedCollectionCapacity,
      context: &mut TypeResolutionContext<'_>,
      string_table: &mut StringTable,
  ) -> TypeResolutionResult<usize>
  ```

- [ ] Reuse existing expression parser and constant folding machinery.
- [ ] Use an expected `Int` context when parsing/folding capacity expressions.
- [ ] Reject:
  - [ ] non-foldable expressions;
  - [ ] runtime names;
  - [ ] runtime/function calls;
  - [ ] non-`Int` values;
  - [ ] zero;
  - [ ] negative values;
  - [ ] values outside `usize`.
- [ ] Store only the folded `usize` in the canonical collection type.
- [ ] Keep source tokens and locations only for diagnostics.

### Diagnostics

Add structured diagnostic reasons. Suggested variants:

```rust
InvalidCollectionTypeReason::ZeroCapacity
InvalidCollectionTypeReason::NegativeCapacity
InvalidCollectionTypeReason::CapacityNotInt
InvalidCollectionTypeReason::CapacityNotConstant
InvalidCollectionTypeReason::CapacityOverflow
InvalidCollectionTypeReason::CapacityOnlyShorthandAmbiguous
InvalidCollectionTypeReason::CapacityOnlyShorthandNotAllowedHere
InvalidCollectionTypeReason::InitializerExceedsFixedCapacity { capacity: usize, length: usize }
InvalidCollectionTypeReason::EmptyImmutableFixedCollection
InvalidCollectionTypeReason::UnexpectedTokenAfterElementType
```

Use `CompilerDiagnostic`, stable diagnostic codes, and precise `SourceLocation`s. Do not create user-facing `CompilerError`s.

### Audit / Style / Validation

- [ ] Confirm capacity folding occurs before RHS expression parsing/type-checking.
- [ ] Confirm no new dependency sorting or AST pass was introduced.
- [ ] Confirm capacity refs use existing dependency mechanics.
- [ ] Confirm diagnostics carry structured facts, not rendered prose.
- [ ] Run:

  ```bash
  cargo test module_dependencies
  cargo test type_resolution
  cargo check
  ```

---

## Phase 4 — Declaration and Literal Semantics

### Context

Collection literal parsing currently receives only an optional element type. It now needs the full receiving collection shape when available.

### Tasks

- [ ] Replace collection literal parser input with a compact expected context.

  Recommended shape:

  ```rust
  pub(crate) enum ExpectedCollectionContext {
      InferGrowable,
      Explicit {
          collection_type_id: TypeId,
          element_type_id: TypeId,
          fixed_capacity: Option<usize>,
      },
      CapacityOnlyShorthand {
          fixed_capacity: usize,
      },
  }
  ```

- [ ] Extend `parse_expectation_for_type_id` or its caller so collection literals can recover `CollectionShape` from expected `TypeId`.
- [ ] Keep unannotated collection literals as growable `{T}`.
- [ ] In an explicit fixed context:
  - [ ] parse elements against the fixed element type;
  - [ ] construct the expression with the fixed collection `TypeId`;
  - [ ] reject literal length greater than capacity.
- [ ] In capacity-only shorthand context:
  - [ ] require the RHS to be an immediate collection literal;
  - [ ] infer the element from the first item;
  - [ ] reject `{}` as ambiguous;
  - [ ] intern the fixed collection type after element inference.
- [ ] Keep empty literal ambiguity for inferred growable declarations.

  ```beanstalk
  items = {} -- invalid
  ```

- [ ] Implement immutable empty fixed binding rejection only for value bindings.
  - Reject `items {64 Int} = {}`.
  - Allow `items ~{64 Int} = {}`.
  - Allow fixed collection field defaults such as `items {64 Int} = {}` inside struct fields.
- [ ] Ensure parameter syntax treats `~` as parameter access mode and `{N T}` as the type shape.
- [ ] Ensure return annotations reject `~{...}`.
- [ ] Ensure aliases reject `~{...}`.
- [ ] Ensure fixed/growable assignment mismatch is rejected through ordinary type compatibility.

### Tests

Positive:

- [ ] inferred growable literal;
- [ ] explicit fixed literal;
- [ ] mutable empty fixed binding;
- [ ] immutable non-empty fixed binding;
- [ ] immutable fixed binding from function call;
- [ ] fixed struct field default;
- [ ] fixed alias;
- [ ] nested fixed collection;
- [ ] generic identity preserves fixed shape;
- [ ] capacity-only shorthand with non-empty literal.

Negative:

- [ ] immutable empty fixed value binding;
- [ ] initializer length greater than fixed capacity;
- [ ] fixed/growable assignment mismatch;
- [ ] exact capacity mismatch;
- [ ] shorthand empty RHS;
- [ ] shorthand with non-literal RHS;
- [ ] shorthand in signature/alias/field/return;
- [ ] `~` in return type;
- [ ] zero/negative/non-`Int`/non-foldable capacity.

### Audit / Style / Validation

- [ ] Confirm no length refinement tracking was added.
- [ ] Confirm empty fixed value rejection is not applied to struct field defaults.
- [ ] Confirm collection literal parsing remains expression-owned.
- [ ] Run:

  ```bash
  cargo test ast::statements::collections
  cargo run -- tests
  cargo check
  ```

---

## Phase 5 — Collection Builtin Method Semantics

### Context

The existing collection builtin parser already has the right mutable receiver rule for `set`, `push`, and `remove`. The main semantic change is making `push` fallible and ensuring receiver access survives AST/HIR lowering.

### Tasks

- [ ] Update `is_fallible_collection_builtin` in `ast/field_access/collection_builtin.rs` to include `Push`.
- [ ] Make `push` return the same internal fallible carrier shape as `set`:
  - success type: builtin unit/none;
  - error type: builtin `Error`.
- [ ] Require `!` or `catch` after every `push` call.
- [ ] Keep `length` infallible.
- [ ] Keep `get`, `set`, and `remove` fallible.
- [ ] Keep `remove` returning the removed element.
- [ ] Keep `set` and `push` with no success value.
- [ ] Audit `NodeKind::CollectionBuiltinCall`.
  - If it currently loses receiver access mode, add an explicit receiver access/effect field.
  - HIR must know mutating receiver calls are mutable/exclusive, not shared.
- [ ] Update HIR lowering of collection builtins.
  - `set`, `push`, and `remove` receiver args must lower as mutable/exclusive call arguments.
  - shared `get` and `length` lower as shared receiver args.
  - mutable `~items.get(index)!` must preserve mutable receiver access for borrow validation if the parser accepts it.
- [ ] Keep `.get()` as value retrieval. Do not add assignment-through-get or indexed assignment sugar.
- [ ] Update `src/libraries/core/collections.rs` metadata/comments if needed so the core package metadata does not contradict fallible `push` semantics.

### Tests

- [ ] `~items.push(1)!` succeeds.
- [ ] `~items.push(1) catch:` succeeds.
- [ ] `~items.push(1)` without handling is rejected.
- [ ] assigning the success result of `push` is rejected.
- [ ] `items.push(1)!` without `~items` is rejected.
- [ ] `items.set(...)` and `items.remove(...)` without `~items` are rejected.
- [ ] shared `items.get(...)!` still works.
- [ ] mutable receiver access is visible to borrow validation.

### Audit / Style / Validation

- [ ] Confirm no user diagnostics are emitted from HIR lowering.
- [ ] Confirm mutating builtins do not lower receiver as shared access.
- [ ] Confirm old `push` examples/tests are updated.
- [ ] Run:

  ```bash
  cargo test collection_builtin
  cargo test borrow
  cargo run -- tests
  cargo check
  ```

---

## Phase 6 — HIR Propagation and Validation

### Context

HIR should remain backend-neutral and TypeId-first. Fixed capacity should flow through `HirExpression.ty` and the module `TypeEnvironment`.

### Tasks

- [ ] Keep `HirExpressionKind::Collection(Vec<HirExpression>)` unless a concrete lowering problem requires a new field.
- [ ] Ensure collection expression `ty` is the exact growable/fixed collection `TypeId`.
- [ ] Add HIR validation that collection expression `ty` resolves to a collection shape.
- [ ] Ensure HIR locals, parameters, returns, and assignments preserve fixed collection `TypeId`s.
- [ ] Update HIR display/debug rendering so fixed collection types print clearly through `display_type`.
- [ ] Ensure reachability and external package validation are unaffected except for `push` now having fallible handling.
- [ ] Ensure HTML-Wasm either:
  - accepts fixed collection TypeIds as inert frontend facts; or
  - rejects only reachable fixed collection operations it cannot lower with structured diagnostics.

### Audit / Style / Validation

- [ ] Confirm there is no HIR capacity side table.
- [ ] Confirm HIR remains parse-syntax-free.
- [ ] Confirm backends recover fixed capacity only through `TypeEnvironment` queries.
- [ ] Run:

  ```bash
  cargo test hir
  cargo run -- tests
  cargo check
  ```

---

## Phase 7 — JavaScript Backend and Runtime

### Context

JS currently lowers all collections to arrays and runtime helpers assume arrays. Fixed capacity must be enforced at runtime.

### Tasks

- [ ] Add runtime helper functions in `src/backends/js/runtime/collections.rs`.

  Suggested helper surface:

  ```js
  function __bs_fixed_collection(items, fixedCapacity) { ... }
  function __bs_collection_items(collection) { ... }
  function __bs_collection_fixed_capacity(collection) { ... }
  function __bs_collection_is_valid(collection) { ... }
  ```

- [ ] Update collection expression lowering in `src/backends/js/js_expr.rs`.
  - Query `TypeEnvironment::collection_shape(expression.ty)`.
  - Growable collection lowers to `[items...]`.
  - Fixed collection lowers to `__bs_fixed_collection([items...], N)`.
- [ ] Update runtime helpers:
  - [ ] accept growable arrays and fixed wrappers;
  - [ ] reject invalid receivers with the existing invalid-collection error path;
  - [ ] make `push` return `{ tag: "ok", value: null }` or an error carrier;
  - [ ] make fixed `push` fail when `items.length >= fixedCapacity`;
  - [ ] optionally wrap growable `push` in `try/catch` to convert JS allocation/runtime failure into `Error!`;
  - [ ] make `get`, `set`, `remove`, and `length` operate on the underlying dense item array;
  - [ ] keep `remove` implemented through dense removal, such as `splice`;
  - [ ] make `length` report item length, not fixed capacity.
- [ ] Add a specific builtin error code if practical.

  Preferred:

  ```rust
  BuiltinErrorCode::CollectionFixedCapacityExceeded
  ```

  Reuse an existing bounds/capacity error only if the user-facing diagnostic remains clear.

- [ ] Update runtime comments so they no longer say `push` is infallible.
- [ ] Keep JS helper logic centralized. Do not inline fixed-capacity checks at every call site.

### Tests

- [ ] Growable collection JS output still uses arrays.
- [ ] Fixed collection JS output uses the fixed wrapper/helper.
- [ ] Fixed `push` succeeds up to capacity and then returns `Error!`.
- [ ] Growable `push` requires source-level handling.
- [ ] `remove` from fixed collection allows a later `push` up to the same max capacity.
- [ ] `length` reports logical length before and after `push`/`remove`.

### Audit / Style / Validation

- [ ] Confirm fixed semantics are enforced at runtime, not only by frontend checks.
- [ ] Confirm JS remains readable and low-noise.
- [ ] Confirm existing growable collection tests still pass.
- [ ] Run:

  ```bash
  cargo test js
  cargo run -- tests
  cargo check
  ```

---

## Phase 8 — Documentation, Matrix, and Roadmap

### Context

Docs must reflect the final design. The old roadmap wording around “capacity” may imply growable allocation hints; update it to fixed collection type constraints.

### Tasks

- [ ] Update `docs/language-overview.md`.
  - [ ] Use “collection” and “fixed collection.”
  - [ ] Define `{T}` and `{N T}`.
  - [ ] Explain `~` as binding/access mode.
  - [ ] Document capacity expressions.
  - [ ] Document capacity-only shorthand restrictions.
  - [ ] Document fixed/growable incompatibility.
  - [ ] Document exact capacity matching.
  - [ ] Document empty immutable fixed binding diagnostics.
  - [ ] Document `push` as fallible.
  - [ ] Document dense semantics: `set` replaces only existing indexes; `push` appends; `remove` shifts.
- [ ] Update user-facing docs under `docs/src/docs/**`.
  - [ ] Collections section/page.
  - [ ] Examples using `push`.
  - [ ] Any terminology saying mutable/immutable collection where it means growable/fixed.
- [ ] Update `docs/src/docs/progress/#page.bst`.
  - [ ] Mark fixed collection type constraints as implemented once code lands.
  - [ ] Record coverage level.
  - [ ] Mention JS runtime fixed-capacity enforcement.
  - [ ] Mention deferred growable initial capacity hints.
  - [ ] Mention deferred default-fill syntax exploration.
- [ ] Update `docs/roadmap/roadmap.md`.
  - [ ] Replace old collection capacity item with the implemented fixed collection type constraint item.
  - [ ] Add concise deferred note for default-fill syntax.
  - [ ] Add concise deferred note for explicit fixed/growable conversion through `copy` after cast/copy hardening.
  - [ ] Add concise deferred note for growable initial capacity hints only if future backend work needs them.
- [ ] Update `docs/compiler-design-overview.md` only if useful.
  - Keep it short: fixed capacity is canonical collection `TypeId` shape visible through `TypeEnvironment` and consumed by HIR/backends.
- [ ] Avoid changing memory-management docs unless `.get()` access semantics need clarification.

### Audit / Style / Validation

- [ ] Confirm docs do not call fixed capacity an allocation hint.
- [ ] Confirm docs do not call fixed collections “immutable collections.”
- [ ] Confirm every `push` example handles fallibility.
- [ ] Run:

  ```bash
  cargo run -- tests
  just validate
  ```

---

## Phase 9 — Integration Test Matrix

### Positive Cases

- [ ] `fixed_collection_literal_success`

  ```beanstalk
  items {4 Int} = {1, 2}
  value = items.get(1)!
  ```

- [ ] `fixed_collection_mutable_empty_push_success`

  ```beanstalk
  items ~{2 Int} = {}
  ~items.push(1)!
  ~items.push(2)!
  ```

- [ ] `fixed_collection_push_overflow_catch`

  ```beanstalk
  items ~{1 Int} = {}
  ~items.push(1)!
  ~items.push(2) catch |err|:
      io(err.message)
  ;
  ```

- [ ] `fixed_collection_remove_preserves_capacity`

  ```beanstalk
  items ~{2 Int} = {1, 2}
  removed = ~items.remove(0)!
  ~items.push(3)!
  ```

- [ ] `fixed_collection_const_capacity_success`

  ```beanstalk
  capacity #Int = 4
  items ~{capacity Int} = {}
  ```

- [ ] `fixed_collection_capacity_expression_success`

  ```beanstalk
  base #Int = 2
  items ~{base + 2 Int} = {}
  ```

- [ ] `fixed_collection_shorthand_success`

  ```beanstalk
  capacity #Int = 4
  names {capacity} = {"a", "b"}
  ```

- [ ] `fixed_collection_alias_success`

  ```beanstalk
  capacity #Int = 4
  Names as {capacity String}
  names Names = {"a"}
  ```

- [ ] `fixed_collection_generic_identity_success`

  ```beanstalk
  identity type A |value A| -> A:
      return value
  ;

  items {4 Int} = {1}
  same = identity(items)
  ```

- [ ] `nested_fixed_collection_success`

  ```beanstalk
  rows #Int = 2
  cols #Int = 3
  grid {rows {cols Int}} = {{1}, {2}}
  ```

- [ ] `fixed_collection_struct_field_default_success`

  ```beanstalk
  Buffer = |
      items {4 Int} = {},
  |

  buffer ~= Buffer()
  ~buffer.items.push(1)!
  ```

### Negative Cases

- [ ] immutable fixed empty binding rejected;
- [ ] fixed initializer length greater than capacity rejected;
- [ ] zero capacity rejected in declarations/signatures/aliases;
- [ ] negative capacity rejected;
- [ ] float capacity rejected;
- [ ] non-foldable capacity rejected;
- [ ] runtime function call in capacity rejected;
- [ ] same-file forward constant capacity reference rejected when current const rules reject it;
- [ ] fixed/growable assignment mismatch rejected;
- [ ] exact capacity mismatch rejected;
- [ ] shorthand with empty literal rejected;
- [ ] shorthand with non-literal RHS rejected;
- [ ] shorthand in signature/alias/field/return rejected;
- [ ] `push` without `!` or `catch` rejected;
- [ ] `items.set(...)` without `~items` rejected;
- [ ] `person ~= items.get(0)!` from shared receiver rejected when it requires mutable access;
- [ ] return type `-> ~{64 Int}` rejected.

### Fixture Requirements

- [ ] Use stable `diagnostic_codes` for failure tests.
- [ ] Prefer output assertions for JS/runtime behavior.
- [ ] Avoid `io()` unless runtime output is the behavior under test.
- [ ] Keep multi-file fixtures inside one case folder.

### Validation

- [ ] Run:

  ```bash
  cargo run -- tests
  just validate
  ```

---

## Phase 10 — Final Cleanup and Acceptance Review

### Tasks

- [ ] Remove obsolete `collection_capacity` fields, comments, and tests.
- [ ] Remove old post-element capacity syntax assumptions.
- [ ] Remove compatibility wrappers for old behavior.
- [ ] Confirm all collection type interning goes through `intern_collection`.
- [ ] Confirm fixed capacity is part of canonical `TypeId` identity.
- [ ] Confirm capacity expressions use existing dependency and constant folding paths.
- [ ] Confirm capacity expression diagnostics are structured and source-located.
- [ ] Confirm HIR carries fixed capacity only through `TypeId`.
- [ ] Confirm JS runtime enforces fixed capacity.
- [ ] Confirm docs, roadmap, and matrix match the implementation.
- [ ] Run final validation.

  ```bash
  just validate
  ```

## 6. Final Acceptance Criteria

- [ ] `{T}` compiles as a growable collection type.
- [ ] `{N T}` compiles as a fixed collection type with exact capacity `N`.
- [ ] Capacity accepts folded compile-time `Int` expressions.
- [ ] Capacity expressions create top-level constant dependency edges where required.
- [ ] Body-local capacity expressions can use earlier visible constants.
- [ ] Capacity-only shorthand works only for valid declaration literal contexts.
- [ ] Fixed capacity is part of canonical `TypeId` identity.
- [ ] Fixed/growable implicit conversion is rejected.
- [ ] Exact capacity mismatch is rejected.
- [ ] Generic inference preserves fixed collection shape.
- [ ] Type aliases preserve fixed collection shape.
- [ ] Return signatures reject `~`.
- [ ] Immutable empty fixed value bindings are rejected.
- [ ] Mutable empty fixed bindings are accepted.
- [ ] Fixed collection fields may have empty defaults and mutate through mutable owner paths.
- [ ] `push` is fallible for all collections.
- [ ] Fixed `push` fails at capacity in JS runtime.
- [ ] `set` only replaces existing indexes.
- [ ] `remove` returns the removed element and preserves fixed max capacity.
- [ ] JS lowering distinguishes growable arrays from fixed collection wrappers.
- [ ] Documentation uses “collection” and “fixed collection.”
- [ ] Roadmap/matrix include deferred notes for default-fill syntax, growable capacity hints, and explicit conversion.
- [ ] `just validate` passes.
