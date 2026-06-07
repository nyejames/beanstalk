# Beanstalk Hash Maps Implementation Plan

**Goal:** first-class, insertion-ordered, safe-by-default hash maps in Beanstalk.  
**V1 runtime target:** HTML JavaScript backend.  
**V1 unsupported target:** HTML-Wasm, rejected by reachable backend-feature validation.  
**Primary implementation rule:** maps are compiler-owned language constructs, not source-library APIs and not external package calls.

Each phase is intended to fit in one coding-agent context window. Every phase ends with a repo audit, style-guide review, and validation step.

---

## 1. Final language surface

### 1.1 Syntax

```bst
scores ~= {"Ada" = 10, "Grace" = 12} -- inferred {String = Int}
empty_scores ~{String = Int} = {}

score = scores.get("Ada") catch:
    then 0
;

~scores.set("Linus", 7) catch:
;

removed = ~scores.remove("Grace") catch:
    then 0
;

if scores.contains("Ada"):
    io("found")
;

count = scores.length
~scores.clear()
```

Type syntax:

```bst
{KeyType = ValueType}
```

Literal syntax:

```bst
{key_expression = value_expression}
```

Rules:

- `{String = Int}` is an insertion-ordered hashmap from `String` keys to `Int` values.
- Any top-level `=` entry inside a `{...}` value literal makes the whole literal a hashmap literal.
- Every hashmap literal entry must be a key/value pair.
- Empty map literals require an explicit/contextual map type.
- Bare identifiers in key position are ordinary variable references, not string-key shorthand.
- No `Map`, `HashMap`, `Dict`, or `Key` source keyword/type is added.
- No fixed/capacity hashmap syntax exists in V1.

Invalid examples:

```bst
bad = {"a" = 1, 2}
bad = {"a", "b" = 2}
bad = {}
bad {String = Int} = {"a" = 1, "a" = 2} -- duplicate known key
```

### 1.2 Ordering semantics

Built-in hashmaps are insertion-ordered as language semantics.

- First successful insertion determines entry position.
- `set(existing_key, value)` replaces only the value.
- Replacing a value does not move the entry.
- Replacing a value keeps the original stored key.
- `remove(key)` removes the entry from the order.
- Re-inserting a removed key appends a new entry.

This prioritizes deterministic UI/template behavior, safety, and ergonomics over raw lookup speed.

### 1.3 Key and value support

V1 key types:

- `String`
- `Int`
- `Bool`
- `Char`

V1 rejected key types:

- `Float`
- structs
- choices
- ordered collections
- hashmaps
- dynamic trait values
- functions
- external opaque types
- templates as a distinct key type
- generic parameters without a future `HASHABLE` bound

Values follow the same runtime-storable rules as ordinary collection elements. Only keys are hashability-restricted.

```bst
users {String = User} = {}
labels {String = String?} = {}
groups {String = {String}} = {}
nested {String = {String = Int}} = {}
```

### 1.4 Built-in member surface

| Surface | Result | Fallible | Receiver access | Notes |
|---|---:|---:|---|---|
| `map.get(key)` | `Value` | yes, `Error!` | shared | returns shared access to stored value |
| `map.contains(key)` | `Bool` | no | shared | use when absence is expected |
| `~map.set(key, value)` | unit | yes, `Error!` | mutable | insert or replace value |
| `~map.remove(key)` | `Value` | yes, `Error!` | mutable | removes key and returns owned value |
| `map.length` | `Int` | no | shared | read-only property |
| `~map.clear()` | unit | no | mutable | removes all entries |

`get`, `set`, and `remove` must be handled with postfix `!` or `catch:`. Hashmap literals are not user-visible fallible expressions.

### 1.5 Ownership and borrow semantics

- Maps own stored keys and values.
- Inserted keys/values follow normal move/copy rules.
- `get`, `contains`, and `remove` borrow the lookup key argument.
- `get` returns shared access into the map.
- While a shared value returned from `get` is live, map mutation is rejected.
- `remove` returns an owned removed value.
- `set` drops/replaces the old stored value without returning it.
- Users who need an old value should call `remove` first, then `set`.

### 1.6 Explicit non-goals

Do not implement these in V1:

- first-class hashsets;
- user-defined hashers/comparers;
- `Float`, struct, choice, collection, or map keys;
- generic key maps before `HASHABLE`;
- map equality;
- map iteration;
- mutable entry APIs;
- indexing syntax;
- const hashmaps;
- display/debug display;
- fixed/capacity maps;
- optimized map variants;
- Wasm map lowering/runtime support.

---

## 2. Current repo anchor

Relevant current repo shape:

```text
src/compiler_frontend/
  datatypes/{ids.rs, definitions.rs, environment.rs, parsed.rs, display.rs, queries.rs}
  declaration_syntax/type_syntax/{mod.rs, parse.rs, walk.rs}
  builtins/{mod.rs, expression_parsing.rs, error_codes.rs, error_type.rs}
  ast/{ast_nodes.rs, statements/collections.rs, field_access/*}
  hir/{expressions.rs, statements.rs, hir_expression.rs, hir_expression/calls.rs, reachability.rs}
  analysis/borrow_checker/{metadata.rs, transfer.rs, transfer/call_semantics.rs}

src/backends/
  backend_feature_validation.rs
  js/{js_expr.rs, js_statement.rs, runtime/*}
  wasm/*

docs/{language-overview.md, compiler-design-overview.md, memory-management-design.md, roadmap/roadmap.md}
docs/src/docs/progress/#page.bst
tests/cases/manifest.toml
```

Current implementation facts to preserve:

- `TypeEnvironment` owns canonical type identity. `DataType` remains parse/diagnostic spelling only.
- Constructed types already use `BuiltinTypeConstructor` plus canonical `TypeId` arguments.
- Ordered collection types currently use `Collection { fixed_capacity }`; maps should be a separate constructed type, not another collection capacity form.
- Collection built-in receiver calls currently lower through external helper IDs. Maps must not copy this pattern.
- JS runtime helpers are already split by semantic group; maps should get a focused helper group.
- Backend feature validation already uses HIR reachability to reject unsupported Wasm features; maps should follow that model.

Phase 0 must refresh these assumptions against the implementation branch before coding. Collection syntax and `copy`/casting changes are expected to land before map work starts.

---

## 3. Implementation strategy

### 3.1 Simplifying choices

Use these constraints to reduce complexity:

- Add one structural map type constructor: `BuiltinTypeConstructor::OrderedMap`.
- Add one map key capability helper. Do not scatter scalar checks across parser, AST, HIR, and backend.
- Generalize the existing curly-literal entry point into a small `ExpectedCurlyLiteralContext`; do not create two unrelated collection/map literal parsers.
- Add one map operation metadata owner under `builtins`; do not duplicate method names, arity, fallibility, and receiver access in several files.
- Add first-class HIR map literal/op nodes; do not add map `ExternalFunctionId`s.
- Reuse existing systems: contextual coercion, fallible carrier handling, receiver access validation, builtin argument parsing, HIR reachability, JS runtime helper emission, and stable diagnostic infrastructure.
- Do not refactor ordered collections to first-class HIR operations in this feature unless the current branch has already moved them there. If the map implementation exposes a clean shared helper, extract only that helper.

### 3.2 Suggested shared compiler structs

Use named structs/enums instead of long argument lists.

```rust
pub(crate) struct MapShape {
    pub(crate) key_type: TypeId,
    pub(crate) value_type: TypeId,
}

pub(crate) enum MapKeyCapability {
    SupportedBuiltin,
    GenericRequiresHashableTrait,
    UnsupportedType,
}

pub(crate) struct MapLiteralEntry {
    pub(crate) key: Expression,
    pub(crate) value: Expression,
}

pub(crate) enum MapBuiltinOp {
    Get,
    Contains,
    Set,
    Remove,
    Clear,
    Length,
}

pub(crate) struct HirMapEntry {
    pub(crate) key: HirExpression,
    pub(crate) value: HirExpression,
}

pub(crate) enum HirMapOp {
    Get,
    Contains,
    Set,
    Remove,
    Clear,
    Length,
}
```

---

## 4. Phase 0 — Current-branch audit

### Context

Run this after fixed/growable collection syntax and `copy`/casting changes land. The implementation must target the final branch shape, not this document’s sampled state.

### Checklist

- [ ] Pull/fetch the latest implementation branch.
- [ ] Record branch name and commit hash in the PR notes.
- [ ] Run `just validate` before changing code.
- [ ] Re-open and inspect:
  - [ ] `src/compiler_frontend/datatypes/ids.rs`
  - [ ] `src/compiler_frontend/datatypes/environment.rs`
  - [ ] `src/compiler_frontend/datatypes/parsed.rs`
  - [ ] `src/compiler_frontend/declaration_syntax/type_syntax/parse.rs`
  - [ ] `src/compiler_frontend/builtins/mod.rs`
  - [ ] `src/compiler_frontend/builtins/expression_parsing.rs`
  - [ ] `src/compiler_frontend/ast/statements/collections.rs`
  - [ ] `src/compiler_frontend/ast/field_access/mod.rs`
  - [ ] `src/compiler_frontend/ast/field_access/collection_builtin.rs`
  - [ ] `src/compiler_frontend/hir/expressions.rs`
  - [ ] `src/compiler_frontend/hir/statements.rs`
  - [ ] `src/compiler_frontend/hir/hir_expression.rs`
  - [ ] `src/compiler_frontend/hir/hir_expression/calls.rs`
  - [ ] `src/compiler_frontend/hir/reachability.rs`
  - [ ] `src/compiler_frontend/analysis/borrow_checker/transfer.rs`
  - [ ] `src/compiler_frontend/analysis/borrow_checker/transfer/call_semantics.rs`
  - [ ] `src/backends/backend_feature_validation.rs`
  - [ ] `src/backends/js/runtime/mod.rs`
  - [ ] `src/backends/js/runtime/collections.rs`
- [ ] Confirm final collection type syntax and update map parser integration points.
- [ ] Confirm current property/member parsing support.
  - [ ] Maps use `map.length` property-style even if ordered collections still use `length()`.
  - [ ] Do not migrate ordered collection `length()` in this feature unless the branch already did.
- [ ] Confirm final `copy` and contextual coercion APIs.
  - [ ] Reuse final copy/coercion APIs for map keys/values.
  - [ ] Do not invent a separate map-specific copy system.
- [ ] Confirm where curly literals are currently owned.
  - [ ] Place map literal parsing beside that owner.
- [ ] Confirm whether collection builtins still lower through external calls.
  - [ ] Regardless, maps lower through first-class HIR map ops.

### Phase exit

- [ ] Add a short PR note summarizing branch-shape findings.
- [ ] Reconcile file names/API names in the rest of this plan if needed.
- [ ] Style review: no implementation started against stale assumptions.
- [ ] Validation: `just validate` passes on the base branch.

---

## 5. Phase 1 — Documentation, roadmap, and matrix

### Context

Document the language target before code changes so diagnostics and tests have a stable source of truth.

### Checklist

- [ ] Update `docs/language-overview.md`.
  - [ ] Add a “Hash Maps” subsection near collections.
  - [ ] Document `{K = V}` type syntax and `{key = value}` literals.
  - [ ] Document insertion-order semantics.
  - [ ] Document key restrictions and value support.
  - [ ] Document `get`, `contains`, `set`, `remove`, `length`, `clear`.
  - [ ] Document `map.length` as property-style.
  - [ ] Document missing-key fallibility and `contains` for no-error checks.
  - [ ] Document V1 deferred surfaces.
- [ ] Update `docs/compiler-design-overview.md`.
  - [ ] Add ordered-map type identity under `TypeEnvironment`.
  - [ ] Add map literals/member ops to AST/HIR ownership notes.
  - [ ] Add borrow-validation notes for `get` aliasing and map mutation.
  - [ ] Add JS-supported/Wasm-deferred backend note.
- [ ] Update `docs/memory-management-design.md` only if needed.
  - [ ] Mention maps own stored keys/values and `get` returns shared access.
- [ ] Update `docs/roadmap/roadmap.md`.
  - [ ] Replace the placeholder `Hash Maps` item with a concrete subsection or link.
  - [ ] Add deferred follow-ups: `HASHABLE`, derived/user hashability, `Float` keys, generic keys, iteration, equality, const maps, display/debug display, fixed maps, optimized variants, hashsets, and Wasm runtime/lowering.
- [ ] Update `docs/src/docs/progress/#page.bst`.
  - [ ] Add a hashmaps row.
  - [ ] Mark frontend/HIR/borrow/JS as target surface once implemented.
  - [ ] Mark HTML-Wasm as deferred/unsupported.
  - [ ] List watch points and deferred features.

### Phase exit

- [ ] Docs examples match agreed syntax.
- [ ] Roadmap/matrix mention every deferred feature.
- [ ] Style review: docs remain compiler-facing; tutorials/examples belong in docs-site pages.
- [ ] Validation: run docs checks available in repo, then `just validate` if docs are part of validation.

---

## 6. Phase 2 — Type identity and key capability

### Context

Maps are structural constructed types. Semantic equality is `TypeId` equality from `TypeEnvironment`; parse-era `DataType` must not drive executable semantics.

### Checklist

- [ ] Add `BuiltinTypeConstructor::OrderedMap` in `datatypes/ids.rs`.
- [ ] Store ordered maps as constructed types with exactly two arguments: `[key_type, value_type]`.
- [ ] Add `MapShape` and `TypeEnvironment` APIs:
  - [ ] `intern_map(key_type: TypeId, value_type: TypeId) -> TypeId`
  - [ ] `map_shape(type_id: TypeId) -> Option<MapShape>`
  - [ ] `map_key_type(type_id: TypeId) -> Option<TypeId>`
  - [ ] `map_value_type(type_id: TypeId) -> Option<TypeId>`
  - [ ] `is_map_type(type_id: TypeId) -> bool`
- [ ] Ensure generic substitution handles map value types automatically.
- [ ] Add parsed type variant:

```rust
ParsedTypeRef::Map {
    key: Box<ParsedTypeRef>,
    value: Box<ParsedTypeRef>,
    location: SourceLocation,
}
```

- [ ] Add diagnostic/render spelling for `{Key = Value}`.
- [ ] Update remapping and parsed-ref walking for map key/value type references.
- [ ] Add one map key capability helper.
  - [ ] Accept `String`, `Int`, `Bool`, `Char`.
  - [ ] Reject generic key parameters with a future-`HASHABLE` diagnostic.
  - [ ] Reject all other key types with structured diagnostics.
  - [ ] Keep the helper ready for future trait evidence checks.
- [ ] Add inline nesting readability validation.
  - [ ] Allow `{String = Int}`.
  - [ ] Allow `{String = {String = Int}}`.
  - [ ] Reject deeper inline map nesting with a suggestion to use a type alias.
  - [ ] Named aliases reset readability depth.

### Tests

- [ ] `intern_map` canonicalizes identical key/value pairs.
- [ ] Different key or value types produce different `TypeId`s.
- [ ] Map type display renders `{String = Int}`.
- [ ] Key capability accepts only V1 key types.
- [ ] Generic key type rejection is targeted.
- [ ] Excessive inline nesting suggests a type alias.

### Phase exit

- [ ] Style review: no backend representation in `datatypes`.
- [ ] Style review: no semantic decisions based on `DataType`.
- [ ] Validation: datatype/type-resolution tests and `just validate`.

---

## 7. Phase 3 — Type syntax parsing

### Context

`{K = V}` must be recognized before fixed/growable collection parsing. The parser should split only on a top-level `=` inside the braced type body.

### Checklist

- [ ] Rename/refactor the braced type parser if helpful, for example from `parse_collection_type` to `parse_braced_type_annotation`.
- [ ] Keep existing collection behavior for `{T}`, `{N T}`, and capacity shorthand.
- [ ] Before collection-capacity parsing, scan collected inner tokens for a top-level `TokenKind::Assign`.
- [ ] The scan must ignore nested braces, parentheses, and other nested expression/type delimiters.
- [ ] If one top-level `=` exists, parse as `ParsedTypeRef::Map`.
- [ ] Reject empty key/value sides.
- [ ] Reject multiple top-level `=` tokens.
- [ ] Parse key and value sides with the existing type-slice parser and require exact consumption.
- [ ] Reject any fixed/capacity map syntax with a targeted diagnostic.
- [ ] Thread map type resolution through `ast::type_resolution`.
- [ ] Validate hashability during semantic type resolution, not raw parsing.
- [ ] Update header dependency walking so key/value type references create needed dependency edges.

### Tests

Positive:

- [ ] map aliases;
- [ ] map parameters;
- [ ] map returns;
- [ ] map struct fields;
- [ ] map choice payloads;
- [ ] map values containing collections;
- [ ] map values containing one direct map;
- [ ] map values containing aliased maps.

Negative:

- [ ] `{Float = Int}`;
- [ ] `{User = Int}` where `User` is a struct;
- [ ] `{{String} = Int}`;
- [ ] `{{String = Int} = Int}`;
- [ ] `{String = {String = {String = Int}}}`;
- [ ] generic `{Key = Value}` before `HASHABLE`;
- [ ] fixed/capacity map syntax;
- [ ] malformed `{= V}`, `{K =}`, `{K = V = X}`.

### Phase exit

- [ ] Style review: syntax parsing does not intern semantic map types directly.
- [ ] Style review: diagnostics carry `SourceLocation` and structured reason enums.
- [ ] Validation: parser/type syntax tests and `just validate`.

---

## 8. Phase 4 — Curly literal parsing and AST map literals

### Context

Curly value literals need one entry point that classifies ordered collections vs maps. Avoid a separate parser that duplicates collection literal logic.

### Checklist

- [ ] Replace or wrap current collection literal context with:

```rust
pub(crate) enum ExpectedCurlyLiteralContext {
    Infer,
    Collection(ExpectedCollectionContext),
    Map(ExpectedMapContext),
}
```

- [ ] Keep current collection literal behavior intact.
- [ ] Add AST map literal shape:

```rust
pub(crate) struct MapLiteralEntry {
    pub(crate) key: Expression,
    pub(crate) value: Expression,
}

ExpressionKind::MapLiteral(Vec<MapLiteralEntry>)
```

- [ ] Add `MapExpressionType` input for expression construction.
  - [ ] `key_type_id`
  - [ ] `value_type_id`
  - [ ] `key_diagnostic_type`
  - [ ] `value_diagnostic_type`
  - [ ] `map_type_id: Option<TypeId>`
- [ ] Add `Expression::map_literal_with_type_id` with an input struct.
- [ ] Update expression remapping for map entries.
- [ ] Ensure maps are not foldable.
- [ ] Ensure debug/string helper paths do not render maps.
- [ ] Literal classification:
  - [ ] If expected type is map and literal is empty, parse as empty map.
  - [ ] If expected type is collection and literal is empty, preserve collection behavior.
  - [ ] If expected type is infer and literal is empty, emit ambiguity diagnostic.
  - [ ] For non-empty infer literals, scan the first top-level entry for top-level `=`.
  - [ ] Once classified, every entry must match that shape.
- [ ] Map entry parsing:
  - [ ] Parse key expression up to top-level `=`.
  - [ ] Consume `=`.
  - [ ] Parse value expression up to comma/close-curly.
  - [ ] Reuse normal expression parsing with a `MapKey` trailing policy if needed.
  - [ ] Do not implement object-literal shorthand.
- [ ] Contextual typing:
  - [ ] If expected map type exists, use expected key/value types.
  - [ ] Otherwise infer key/value from the first entry.
  - [ ] Coerce every key and value through existing contextual coercion.
  - [ ] `none` in value position uses the map value type context.
  - [ ] String literal/slice/template key expressions use normal string coercion.
- [ ] Validate key capability once key type is known.
- [ ] Reject map literals in const contexts.
- [ ] Duplicate key detection:
  - [ ] Add `KnownMapKey` for foldable `String`, `Int`, `Bool`, `Char` keys.
  - [ ] Detect duplicates after coercion where cheaply knowable.
  - [ ] Do not add broad constant folding for this feature.
  - [ ] Runtime duplicate keys remain valid and use `set` semantics.

### Tests

Positive:

- [ ] inferred non-empty literal;
- [ ] explicit empty literal;
- [ ] runtime key expression;
- [ ] bare identifier key is a variable;
- [ ] contextual `none` value;
- [ ] string key coercion;
- [ ] nested map value;
- [ ] map type alias literal.

Negative:

- [ ] empty map without type context;
- [ ] mixed collection/map entries;
- [ ] duplicate known key;
- [ ] unknown bare identifier key;
- [ ] unhashable key expression;
- [ ] const map literal.

### Phase exit

- [ ] Style review: no user-input panic paths.
- [ ] Style review: no copied collection parser with diverging logic.
- [ ] Validation: AST literal tests and `just validate`.

---

## 9. Phase 5 — Built-in map members in AST

### Context

Map members are compiler-owned builtins. They are not user receiver methods and not importable/free functions.

### Checklist

- [ ] Add `src/compiler_frontend/builtins/maps.rs` or equivalent focused metadata owner.
- [ ] Add `MapBuiltinOp` and one metadata function that returns:
  - [ ] source member name;
  - [ ] arity;
  - [ ] receiver access requirement;
  - [ ] fallibility;
  - [ ] success type policy.
- [ ] Add `src/compiler_frontend/ast/field_access/map_builtin.rs`.
- [ ] Wire map builtin parsing into the postfix/member coordinator after receiver type is known.
- [ ] Dispatch only when `TypeEnvironment::map_shape(receiver_type_id)` succeeds.
- [ ] Reuse existing receiver access validation.
- [ ] Reuse existing builtin positional argument parser where possible.
- [ ] Implement surface rules:
  - [ ] `get(key)` — shared, fallible, returns value carrier.
  - [ ] `contains(key)` — shared, infallible, returns `Bool`.
  - [ ] `set(key, value)` — mutable, fallible, returns unit carrier.
  - [ ] `remove(key)` — mutable, fallible, returns value carrier.
  - [ ] `clear()` — mutable, infallible, returns unit.
  - [ ] `length` — shared, infallible, property-style, returns `Int`.
- [ ] Add AST node:

```rust
NodeKind::MapBuiltinCall {
    receiver: Box<AstNode>,
    op: MapBuiltinOp,
    receiver_requires_mutable: bool,
    args: Vec<CallArgument>,
    result_type_ids: Vec<TypeId>,
    location: SourceLocation,
}
```

- [ ] Require fallible handling for `get`, `set`, and `remove`.
- [ ] Reject `!`/`catch` on `contains`, `clear`, and `length`.
- [ ] Reject `map.length = ...`.
- [ ] Reject assignment through `map.get(...)`.
- [ ] Do not add map external function IDs.

### Tests

- [ ] all valid members parse and type-check;
- [ ] `map.length` property works;
- [ ] `map.length()` rejected with a useful diagnostic;
- [ ] fallible map ops require handling;
- [ ] infallible map ops reject fallible handling;
- [ ] mutable map ops require `~`;
- [ ] immutable map bindings cannot be mutated;
- [ ] map builtins are not callable as free functions;
- [ ] user receiver methods cannot override map builtins.

### Phase exit

- [ ] Style review: one metadata source for op shape; no repeated name/arity tables.
- [ ] Style review: user diagnostics use `CompilerDiagnostic`.
- [ ] Validation: receiver/member tests and `just validate`.

---

## 10. Phase 6 — HIR representation and lowering

### Context

Maps must be explicit in HIR so borrow validation and Wasm feature validation can understand them directly.

### Checklist

- [ ] Add HIR map literal data:

```rust
HirExpressionKind::MapLiteral(Vec<HirMapEntry>)
```

- [ ] Add HIR map operation statement:

```rust
HirStatementKind::MapOp {
    op: HirMapOp,
    receiver: HirExpression,
    args: Vec<HirExpression>,
    result: Option<LocalId>,
}
```

- [ ] Use `MapOp` for all map members, including `length`, so borrow/reachability/backend lowering has one operation path.
- [ ] Update HIR display/debug formatting.
- [ ] Update HIR validation.
- [ ] Update side-table source mapping for map operations.
- [ ] Lower `ExpressionKind::MapLiteral` to `HirExpressionKind::MapLiteral`.
  - [ ] Lower keys/values in source order.
  - [ ] Preserve prelude order.
  - [ ] Produce an rvalue of the canonical map type.
- [ ] Lower `NodeKind::MapBuiltinCall` to `HirStatementKind::MapOp`.
  - [ ] Lower receiver as shared/mutable based on op.
  - [ ] Lower arguments in source order.
  - [ ] Allocate a temp local for result-bearing ops.
  - [ ] Return a load of the result local into existing expression/fallible handling.
- [ ] Ensure `get`, `set`, and `remove` produce the existing fallible carrier shape.
- [ ] Ensure literals remain infallible source expressions.
- [ ] Update fallible handling only where it assumes fallible values come only from user/external calls.
- [ ] Extend HIR reachability with reachable map operations/literals and locations.

### Tests

- [ ] HIR map literal preserves entry order.
- [ ] HIR `get` produces a result local with fallible carrier type.
- [ ] HIR `set` uses mutable receiver and key/value args.
- [ ] HIR `length` lowers as map op and returns `Int`.
- [ ] HIR reachability records reachable map construction/use.
- [ ] Unreachable map helper functions follow the existing reachability policy.

### Phase exit

- [ ] Style review: map HIR ops are not external calls.
- [ ] Style review: HIR user-facing failures were rejected earlier.
- [ ] Validation: HIR tests and `just validate`.

---

## 11. Phase 7 — Borrow validation

### Context

JS would appear to work without this, but Beanstalk semantics require `get` to be a shared alias into the map and mutation to be rejected while that alias is live.

### Checklist

- [ ] Add transfer handling for `HirStatementKind::MapOp`.
- [ ] Reuse existing borrow vocabulary where possible:
  - [ ] shared borrow;
  - [ ] mutable borrow;
  - [ ] may-consume inserted values;
  - [ ] fresh result;
  - [ ] alias result.
- [ ] Effects:
  - [ ] `get`: receiver shared, key shared, result aliases receiver root conservatively.
  - [ ] `contains`: receiver shared, key shared, result fresh.
  - [ ] `set`: receiver mutable borrow, key may-consume, value may-consume, result fresh unit/error carrier.
  - [ ] `remove`: receiver mutable borrow, key shared, result fresh owned value/error carrier.
  - [ ] `clear`: receiver mutable borrow, result fresh unit.
  - [ ] `length`: receiver shared, result fresh `Int`.
- [ ] Update expression-root traversal for `HirExpressionKind::MapLiteral`.
  - [ ] Keys and values are owned into the fresh map construction.
  - [ ] Reuse collection literal move/copy behavior where it already exists.
  - [ ] Stored children must not become aliases of their source places.
- [ ] Keep aliasing conservative: no per-key/per-entry alias tracking in V1.
- [ ] Add side-table facts for map ops if needed by later ownership lowering.

### Tests

- [ ] `get` result blocks later `set` while live.
- [ ] `get` result blocks later `remove` while live.
- [ ] `get` result blocks later `clear` while live.
- [ ] `contains` and `length` do not create long-lived aliases beyond ordinary expression use.
- [ ] `remove` result is owned and can outlive later map mutation.
- [ ] `set` consumes inserted non-copy values unless `copy` is used.
- [ ] map literal consumes inserted non-copy values unless `copy` is used.
- [ ] lookup keys for `get`, `contains`, and `remove` are not consumed.

### Phase exit

- [ ] Style review: borrow checker does not mutate HIR.
- [ ] Style review: backend/runtime code is not consulted by borrow analysis.
- [ ] Validation: borrow tests and `just validate`.

---

## 12. Phase 8 — JavaScript backend and runtime

### Context

HTML/JS V1 uses a small Beanstalk runtime wrapper around native JavaScript `Map`. The wrapper centralizes validation, errors, and future replacement.

### Runtime representation

```js
{
  __bst_kind: "ordered_map",
  map: new Map()
}
```

### Checklist

- [ ] Add `src/backends/js/runtime/maps.rs`.
- [ ] Emit map helpers from `runtime/mod.rs`.
- [ ] Add runtime helpers:
  - [ ] `__bs_map_new(entries)`
  - [ ] `__bs_map_is_valid(value)`
  - [ ] `__bs_map_get(map, key)`
  - [ ] `__bs_map_contains(map, key)`
  - [ ] `__bs_map_set(map, key, value)`
  - [ ] `__bs_map_remove(map, key)`
  - [ ] `__bs_map_clear(map)`
  - [ ] `__bs_map_length(map)`
- [ ] Add built-in runtime error codes:
  - [ ] `MapKeyNotFound`
  - [ ] `MapExpectedOrderedMap`
- [ ] Error messages must be deterministic and must not render arbitrary keys.
- [ ] `__bs_map_new(entries)` inserts in source order.
  - [ ] Runtime duplicate keys update value and keep first insertion position.
- [ ] `get`, `set`, and `remove` return fallible carriers.
- [ ] `contains`, `clear`, and `length` are infallible helpers.
- [ ] Lower `HirExpressionKind::MapLiteral` to `__bs_map_new([[key, value], ...])` or equivalent.
- [ ] Lower `HirStatementKind::MapOp` to runtime helper calls.
- [ ] Audit explicit `copy` behavior.
  - [ ] If current collection copy performs deep value copy, add map copy parity.
  - [ ] If current copy traits/design reject unsupported compound copies, reject map copy consistently.
  - [ ] Do not add a standalone map-specific copy model.
- [ ] Ensure maps are non-displayable.
  - [ ] Reject template interpolation and `io(map)` before backend lowering where possible.
  - [ ] Avoid JS fallback output such as `[object Map]`.

### Tests

- [ ] JS helper group is emitted.
- [ ] literal + `get` produces expected HTML output.
- [ ] missing `get` recovers through `catch`.
- [ ] `contains` true/false works.
- [ ] `set` insert increases length.
- [ ] `set` replacement preserves length.
- [ ] `remove` returns old value and decreases length.
- [ ] `clear` resets length.
- [ ] runtime duplicate key in literal uses latest value and length one.
- [ ] explicit map copy behavior matches current copy policy.

### Phase exit

- [ ] Style review: helper group is focused; no monolithic JS runtime growth.
- [ ] Style review: JS backend stays GC-semantic and does not pretend ownership lowering exists.
- [ ] Validation: JS backend tests, HTML integration tests, `just validate`.

---

## 13. Phase 9 — HTML-Wasm unsupported-feature validation

### Context

Map semantics are frontend/HIR-valid, but Wasm runtime support is deferred. Reachable use must fail before Wasm lowering/byte emission.

### Checklist

- [ ] Extend HIR reachability result with reachable map uses.
  - [ ] Include construction vs operation kind.
  - [ ] Include source location.
- [ ] Extend `validate_hir_backend_feature_support`.
  - [ ] For `BackendTarget::Wasm`, reject first reachable map use.
  - [ ] Use `CompilerDiagnostic::unsupported_backend_feature` unless a more specific structured diagnostic exists.
  - [ ] Feature labels: `hashmap construction`, `hashmap operation`, or equivalent stable text.
- [ ] Preserve unreachable-helper behavior if current reachability supports it.
- [ ] Do not add partial Wasm map lowering in V1.

### Tests

- [ ] HTML backend succeeds for canonical map cases.
- [ ] HTML-Wasm rejects reachable map literal.
- [ ] HTML-Wasm rejects reachable map operation.
- [ ] HTML-Wasm allows unreachable map helper if existing policy allows unreachable dynamic-trait helpers.
- [ ] Diagnostics include target and feature.

### Phase exit

- [ ] Style review: rejection is user-facing `CompilerDiagnostic`, not backend `CompilerError`.
- [ ] Validation: backend feature validation tests and `just validate`.

---

## 14. Phase 10 — Diagnostics and integration coverage

### Context

This phase hardens the feature for agent and user workflows. Prefer integration tests for user-facing behavior; keep unit tests for type/HIR/borrow invariants.

### Diagnostic checklist

Add structured diagnostic reasons/constructors for:

- [ ] invalid map type syntax;
- [ ] unsupported fixed/capacity map syntax;
- [ ] unsupported key type;
- [ ] generic key requiring future `HASHABLE`;
- [ ] excessive inline map type nesting;
- [ ] empty map literal ambiguity;
- [ ] mixed collection/map literal entries;
- [ ] missing key/value expression in map literal entry;
- [ ] duplicate known literal key;
- [ ] const map literal deferred;
- [ ] map equality deferred;
- [ ] map iteration deferred;
- [ ] map display unavailable;
- [ ] `map.length` assignment invalid;
- [ ] fallible map operation must be handled;
- [ ] infallible map operation cannot use `!`/`catch`;
- [ ] map mutation requires mutable receiver;
- [ ] map method/property arity or shape mismatch.

Diagnostic requirements:

- [ ] Use `CompilerDiagnostic` for user-facing failures.
- [ ] Use stable diagnostic codes in integration failures.
- [ ] Carry `TypeId`s for type diagnostics.
- [ ] Render type names through diagnostic render context.
- [ ] Include precise source locations.
- [ ] Add suggestions where useful:
  - [ ] add explicit map type for `{}`;
  - [ ] use a type alias for nested maps;
  - [ ] use `contains(key)` for absence checks;
  - [ ] use `copy value` if needed after insertion.

### Integration cases

Add cases under `tests/cases/` and register them in `tests/cases/manifest.toml`.

Positive HTML/JS cases:

- [ ] literal lookup;
- [ ] mutation: `set`, replace, `remove`, `clear`, `length`;
- [ ] `contains` true/false;
- [ ] contextual typing: empty explicit map, `none` value, string key coercion;
- [ ] aliases, struct fields, function parameter/return maps;
- [ ] runtime key expressions;
- [ ] nested map/collection values.

Borrow cases:

- [ ] `get` blocks mutation while alias is live;
- [ ] `remove` result is owned;
- [ ] insert/literal move-copy rules.

Negative cases:

- [ ] empty inferred map;
- [ ] mixed literal entries;
- [ ] duplicate known literal key;
- [ ] unhashable key;
- [ ] generic key before `HASHABLE`;
- [ ] const map literal;
- [ ] template/`io` display;
- [ ] equality;
- [ ] iteration;
- [ ] fixed/capacity map type;
- [ ] excessive inline nesting;
- [ ] missing `catch`/`!`;
- [ ] mutation without `~`;
- [ ] assignment to `map.length`.

Backend matrix cases:

- [ ] HTML success for positive map cases.
- [ ] HTML-Wasm unsupported diagnostic for reachable map construction.
- [ ] HTML-Wasm unsupported diagnostic for reachable map operation.
- [ ] HTML-Wasm unreachable-map helper behavior if supported.

### Phase exit

- [ ] Style review: diagnostics are structured and stable.
- [ ] Style review: tests assert behavior, not incidental block indexes or generated JS formatting.
- [ ] Validation: `cargo run -- tests` and `just validate`.

---

## 15. Phase 11 — Final docs, style, and PR closeout

### Checklist

- [ ] Re-read changed docs and compare against implementation.
- [ ] Confirm docs agree on:
  - [ ] `{K = V}` type syntax;
  - [ ] `{key = value}` literal syntax;
  - [ ] insertion order;
  - [ ] key restrictions;
  - [ ] receiver members;
  - [ ] fallibility;
  - [ ] JS support;
  - [ ] Wasm deferral;
  - [ ] deferred features.
- [ ] Confirm roadmap and matrix list every deferred item.
- [ ] Run style-guide audit:
  - [ ] clear module ownership;
  - [ ] no user-input panics;
  - [ ] no obsolete compatibility wrappers;
  - [ ] no semantic `DataType` comparisons;
  - [ ] no user diagnostics through `CompilerError`;
  - [ ] no broad map-specific duplication of existing collection/coercion/fallibility systems;
  - [ ] no dense clever iterator chains in validation logic;
  - [ ] tests live outside production files.
- [ ] Run:

```bash
just validate
```

- [ ] Add PR summary:
  - [ ] supported surface;
  - [ ] intentionally deferred features;
  - [ ] backend support status;
  - [ ] validation commands run;
  - [ ] follow-up tickets.

---

## 16. Suggested agent chunk split

| Chunk | Phases | Merge condition |
|---|---|---|
| 1 | 0–3 | docs updated; map types parse/resolve/display; negative type diagnostics work |
| 2 | 4–5 | map literals and map members type-check into AST; no backend lowering yet |
| 3 | 6–7 | HIR and borrow validation model map literals/ops correctly |
| 4 | 8–9 | HTML/JS maps run; HTML-Wasm rejects reachable map use cleanly |
| 5 | 10–11 | diagnostics, integration matrix, docs, and full validation complete |

Do not merge a chunk that accepts source syntax but silently mis-lowers it for any selected backend.

---

## 17. Roadmap text to add

```markdown
## Hash Maps

Implement first-class insertion-ordered hashmaps with `{Key = Value}` type syntax and `{key = value}` literals.
The Alpha implementation targets frontend parsing/type-checking, HIR, borrow validation, and the HTML JavaScript backend.
HTML-Wasm support is deferred until the Wasm runtime feature pass.

V1 supports only compiler-known scalar key types: `String`, `Int`, `Bool`, and `Char`.
Generic keys, user-defined keys, and derived hashability require a future `HASHABLE` trait/capability.

Deferred follow-ups:
- `HASHABLE` trait and generic key maps
- derived/user-defined hashability
- `Float` key policy
- map iteration
- map equality semantics
- const hashmaps
- map display / `DISPLAYABLE` / `DEBUG_DISPLAY`
- fixed/capacity-specialized maps
- optimized map variants
- hashsets in core/std library
- HTML-Wasm map runtime/lowering
```

Suggested implementation-matrix watch point:

```markdown
Hash maps | Supported after implementation | Targeted/Broad | JS / HTML; Wasm deferred | Insertion-ordered maps with `{K = V}` type syntax and `{key = value}` literals. V1 supports `String`, `Int`, `Bool`, and `Char` keys only. `get`, `set`, and `remove` are fallible. `contains`, `length`, and `clear` are infallible. Iteration, equality, const maps, display, hashsets, generic keys, user-defined hashability, fixed maps, optimized variants, and Wasm lowering remain deferred.
```

---

## 18. Completion definition

Hashmaps are complete for V1 when:

- [ ] `{K = V}` type syntax parses, resolves, displays, aliases, imports, and diagnoses correctly.
- [ ] `{key = value}` literals infer and type-check correctly.
- [ ] Empty map literals require explicit/contextual map type.
- [ ] Supported key types work; unsupported key types produce clear diagnostics.
- [ ] `get`, `contains`, `set`, `remove`, `length`, and `clear` work as specified.
- [ ] `get`, `set`, and `remove` require fallible handling.
- [ ] Map literals are not user-visible fallible expressions.
- [ ] `get` result aliasing is enforced by borrow validation.
- [ ] JS backend emits and uses ordered-map runtime helpers.
- [ ] HTML-Wasm rejects reachable map use with structured unsupported-backend diagnostics.
- [ ] Deferred features are rejected or reserved with diagnostics.
- [ ] Roadmap and implementation matrix explicitly list deferred work.
- [ ] Positive and negative integration tests cover the feature.
- [ ] `just validate` passes.
