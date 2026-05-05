# Beanstalk Type Environment Redesign Plan

## Status

Deferred follow-up for the AST pipeline restructure.

Start this plan only after the AST pipeline refactor has removed the current build-state, declaration-table, constant-resolution, scope-context, and parser churn bottlenecks. This work should not be mixed into the active AST pipeline restructure.

The user will manually place this plan in the roadmap. Do not include an implementation task to replace or move this plan file.

## Summary

Redesign Beanstalk's frontend type environment around compact canonical type IDs.

The current compiler carries large `DataType` payloads through AST, HIR preparation, diagnostics, generic substitution, nominal definitions, and type comparison. This creates repeated cloning, duplicated type identity logic, and a blurred boundary between:

- parsed type syntax
- unresolved type names
- resolved frontend semantic type identity
- nominal type definitions
- generic instantiations
- HIR semantic IR
- backend layout/runtime representation

This plan introduces a table-backed `TypeEnvironment` that owns resolved frontend type identity. AST resolves parsed type syntax into canonical `TypeId`s. HIR carries those `TypeId`s and receives the `TypeEnvironment` at the compiled module boundary. Backend-specific layout, ABI, ownership flags, and runtime representation remain outside the frontend type environment.

## Goals

- Introduce compact type IDs for common frontend type identity.
- Move nominal type definitions into a table instead of repeatedly carrying large `DataType` payloads.
- Intern generic nominal instances so repeated instantiations share one canonical representation.
- Intern constructed builtin types such as `{T}`, `T?`, and `Result<T, E>`.
- Reduce avoidable `DataType` cloning across AST, HIR preparation, type coercion, and diagnostics.
- Use `TypeId` equality for resolved semantic type identity.
- Keep type compatibility policy in `type_coercion`, layered over `TypeId + TypeEnvironment`.
- Separate frontend type identity from backend layout and runtime representation.
- Remove duplicate semantic type interners between AST and HIR.
- Keep AST and HIR boundaries explicit about which stage owns type resolution and which stage owns lowering.

## Non-goals

- Do not change Beanstalk language semantics.
- Do not mix this with the current AST pipeline restructure phases.
- Do not preserve compatibility wrappers for old type APIs.
- Do not keep old `DataType` APIs alive through forwarding shims.
- Do not add a second HIR semantic type interner.
- Do not introduce backend layout IDs as part of this work.
- Do not move Wasm/JS layout, ABI, drop strategy, ownership flags, or runtime representation into `TypeEnvironment`.
- Do not complete the future structured diagnostic enum refactor in this plan.
- Do not fully redesign diagnostics to render `TypeId`s correctly in this pass.
- Do not introduce a package-cache-stable global `TypeId`.

## Design decisions

### Type identity scope

Use module-local dense IDs for fast frontend type identity.

```rust
pub struct TypeId(u32);
```

`TypeId` is valid only with the `TypeEnvironment` that created it.

Deterministic lookup comes from stable keys, not from the numeric ID itself.

Use keys for canonicalization:

```rust
pub enum TypeKey {
    Builtin(BuiltinTypeKey),
    Nominal(NominalTypeId),
    Constructed(ConstructedTypeKey),
    GenericParameter(GenericParameterId),
    Function(FunctionTypeKey),
    External(ExternalTypeId),
}
```

Do not rely on raw `TypeId` stability across builds.

### Semantic identity

Resolved semantic type identity is `TypeId` equality.

```rust
left == right
```

Deep comparison is allowed only in two narrow places:

1. Inside the type interner, to decide whether a structural builtin type already exists.
2. Inside named `TypeEnvironment` queries, when the question is not identity.

Examples:

```rust
type_environment.is_numeric(type_id)
type_environment.collection_element_type(type_id)
type_environment.result_slots(type_id)
type_environment.supports_runtime_equality(type_id)
```

Compatibility remains a policy question:

```rust
type_coercion::is_assignable(actual, expected, type_environment)
```

### Type aliases

Type aliases are transparent and do not create semantic type identity.

Alias resolution returns the canonical target `TypeId`.

Keep written alias spelling only where needed for diagnostics:

```rust
pub struct TypeAnnotationResolution {
    pub type_id: TypeId,
    pub written_type: WrittenType,
}
```

Do not store alias names on every expression node.

### Builtins

Seed builtins when creating `TypeEnvironment`.

Use named accessors instead of magic numeric assumptions:

```rust
type_environment.builtins().int
type_environment.builtins().float
type_environment.builtins().string
type_environment.builtins().error
```

A small fixed set of constants is acceptable only if it improves readability and is tightly documented.

### Parsed types vs resolved types

Do not assign `TypeId` to unresolved names.

Use parsed/resolution-layer types for syntax and partial resolution:

```rust
pub enum ParsedTypeRef {
    Inferred,
    Named {
        name: StringId,
        location: SourceLocation,
    },
    Applied {
        base: Box<ParsedTypeRef>,
        arguments: Vec<ParsedTypeRef>,
        location: SourceLocation,
    },
    Collection {
        element: Box<ParsedTypeRef>,
        location: SourceLocation,
    },
    Optional {
        inner: Box<ParsedTypeRef>,
        location: SourceLocation,
    },
    Result {
        ok: Box<ParsedTypeRef>,
        err: Box<ParsedTypeRef>,
        location: SourceLocation,
    },
}
```

AST resolves `ParsedTypeRef -> TypeId`.

`TypeId` means resolved semantic identity. It must not represent unresolved placeholders.

### Inference placeholders

Remove `DataType::Inferred` as a resolved type concept.

Use a checking/resolution state outside the type table:

```rust
pub enum ExpectedType {
    Known(TypeId),
    Infer,
}
```

or:

```rust
pub enum TypeResolution {
    Explicit(TypeId),
    NeedsInference(SourceLocation),
}
```

Inference is a resolution state, not a semantic type.

### `DataType` replacement

Split the current role of `DataType` into:

- parsed type syntax
- type IDs
- type definitions
- type environment queries
- diagnostic display helpers
- coercion policy

Target model:

```rust
pub struct TypeRef {
    pub id: TypeId,
}

pub enum TypeDefinition {
    Builtin(BuiltinTypeDefinition),
    Struct(StructTypeDefinition),
    Choice(ChoiceTypeDefinition),
    Constructed(ConstructedTypeDefinition),
    Function(FunctionTypeDefinition),
    External(ExternalTypeDefinition),
    GenericParameter(GenericParameterDefinition),
    GenericInstance(GenericInstanceDefinition),
}
```

`DataType` may exist temporarily while threading the migration, but the end state must delete it or reduce it to a non-semantic parsed/syntax type with a new name. Do not keep a compatibility layer.

### Function signatures

Resolved function signatures store `TypeId`s.

Access, mutability, receiver state, return aliasing, and value mode stay separate from type identity.

```rust
pub struct ResolvedFunctionSignature {
    pub parameters: Vec<ResolvedParameter>,
    pub returns: Vec<TypeId>,
    pub error_return: Option<TypeId>,
    pub generic_parameters: GenericParameterListId,
}

pub struct ResolvedParameter {
    pub name: StringId,
    pub type_id: TypeId,
    pub access: ParameterAccessMode,
}
```

### Expressions

Expression nodes carry canonical `TypeId`, not rendered type names or alias spelling.

```rust
pub struct Expression {
    pub kind: ExpressionKind,
    pub type_id: TypeId,
    pub value_mode: ValueMode,
    pub location: SourceLocation,
}
```

Source spelling belongs to parsed type annotations or diagnostic context, not every expression.

### Generic nominal instantiation

Each concrete nominal generic instance interns to a canonical `TypeId`.

Examples:

- `Box of Int`
- `Box of String`
- `Pair of String, Int`

Use a key based on canonical IDs:

```rust
pub struct GenericInstanceKey {
    pub base: NominalTypeId,
    pub arguments: Box<[TypeId]>,
}
```

The cache maps keys to IDs:

```rust
FxHashMap<GenericInstanceKey, TypeId>
```

Do not cache cloned `DataType` payloads.

### Generic substitution

Use lazy substitution by default.

A generic instance stores:

- base nominal definition ID
- concrete argument `TypeId`s
- optional source/debug metadata

Field and variant lookup resolves through `TypeEnvironment` views:

```rust
type_environment.fields_for(instance_type_id)
type_environment.variants_for(instance_type_id)
```

Substitute `T -> Int` only when fields/variants/signatures are queried.

Do not eagerly clone full struct/choice definitions for every instantiation.

### Constructed builtin types

Collections, options, results, and function types use the same constructed-type interning infrastructure.

```rust
pub enum TypeConstructor {
    Builtin(BuiltinTypeConstructor),
    Nominal(NominalTypeId),
    External(ExternalTypeId),
}

pub enum BuiltinTypeConstructor {
    Collection,
    Option,
    Result,
}

pub struct ConstructedTypeKey {
    pub constructor: TypeConstructor,
    pub arguments: Box<[TypeId]>,
}
```

`{Int}`, `Int?`, and `Result<Int, Error>` become interned types, not recursive `DataType` trees.

### Nominal definitions

Struct and choice definitions live in `TypeEnvironment`.

HIR must not keep duplicate nominal registries once the migration is complete.

Nominal definitions should include enough semantic data for AST, HIR, borrow validation, and backend lowering:

```rust
pub struct StructTypeDefinition {
    pub id: NominalTypeId,
    pub path: InternedPath,
    pub fields: Box<[FieldDefinition]>,
    pub generic_parameters: Option<GenericParameterListId>,
    pub const_record: bool,
}

pub struct ChoiceTypeDefinition {
    pub id: NominalTypeId,
    pub path: InternedPath,
    pub variants: Box<[ChoiceVariantDefinition]>,
    pub generic_parameters: Option<GenericParameterListId>,
}
```

### Type coercion boundary

Keep `type_coercion` separate.

`TypeEnvironment` answers facts.

```rust
type_environment.is_numeric(type_id)
type_environment.is_collection(type_id)
type_environment.collection_element_type(type_id)
type_environment.result_slots(type_id)
type_environment.nominal_definition(type_id)
```

`type_coercion` answers policy.

```rust
type_coercion::is_assignable(actual, expected, type_environment)
type_coercion::coerce_numeric_binary_operands(left, right, type_environment)
```

Do not move contextual compatibility rules into `TypeEnvironment`.

### HIR type model

Delete/replace HIR's separate semantic `TypeContext` and HIR `TypeId`.

HIR should carry frontend semantic `TypeId`s from AST lowering.

Do not keep two semantic type interners.

Keep HIR-specific helpers only when they are genuinely about HIR or lowering shape, not identity.

### Compiled module boundary

Store `TypeEnvironment` on the outer compiled module boundary.

Target shape:

```rust
pub struct CompiledModule {
    pub hir: HirModule,
    pub type_environment: TypeEnvironment,
    pub borrow_report: BorrowReport,
    pub constants: ModuleConstants,
    pub page_fragments: Vec<ConstPageFragment>,
}
```

The exact struct name may differ depending on current frontend output types, but the ownership rule is fixed:

- HIR nodes carry compact `TypeId`s.
- The compiled module carries the type table.
- Backends receive both.
- HIR does not own a separate type table.

### Diagnostics boundary

This plan should only do the minimal diagnostic plumbing required to keep existing diagnostics compiling and useful.

Future structured diagnostics are deliberately deferred.

Expected future shape:

```rust
pub enum Diagnostic {
    TypeMismatch {
        location: SourceLocation,
        expected: TypeId,
        found: TypeId,
        context: TypeMismatchContext,
    },
}
```

This redesign should avoid work that will be thrown away by that follow-up.

Allowed in this plan:

- pass `TypeId`s where type errors are created
- add temporary rendering helpers at diagnostic boundaries
- preserve existing message quality where practical

Avoid in this plan:

- redesigning all diagnostic enums
- fully moving compiler errors away from strings
- deeply restructuring diagnostic rendering
- adding elaborate type-name rendering that will be replaced soon

### Backend layout boundary

`TypeEnvironment` stores frontend semantic facts only.

Do not store:

- Wasm layout
- JS runtime representation
- ABI classification
- ownership flags
- drop strategies
- concrete field offsets
- GC metadata layout
- target-specific scalar widths

Use later backend/lowering tables:

```rust
pub struct LayoutEnvironment {
    pub layouts_by_type: FxHashMap<TypeId, LayoutTypeId>,
}
```

Frontend may expose facts lowerings need:

```rust
type_environment.type_kind(type_id)
type_environment.nominal_fields(type_id)
type_environment.choice_variants(type_id)
type_environment.is_heap_managed_candidate(type_id)
```

Final representation belongs to backend/LIR.

## Target module layout

Replace `src/compiler_frontend/datatypes.rs` with a real module directory.

```text
src/compiler_frontend/datatypes/
    mod.rs
    ids.rs
    environment.rs
    definitions.rs
    parsed.rs
    display.rs
    generics.rs
    nominal.rs
    queries.rs
    tests/
```

Suggested ownership:

### `mod.rs`

- public surface
- module documentation
- re-exports
- high-level ownership notes

### `ids.rs`

- `TypeId`
- `NominalTypeId`
- `GenericParameterId`
- `GenericParameterListId`
- helper newtypes

### `environment.rs`

- `TypeEnvironment`
- type storage
- interning entry points
- builtin seeding
- canonical key maps

### `definitions.rs`

- `TypeDefinition`
- `BuiltinTypeDefinition`
- `StructTypeDefinition`
- `ChoiceTypeDefinition`
- `ConstructedTypeDefinition`
- `FunctionTypeDefinition`
- `ExternalTypeDefinition`
- field and variant definition structs

### `parsed.rs`

- `ParsedTypeRef`
- `WrittenType`
- parsed annotation helper structs
- no semantic type identity

### `generics.rs`

- generic parameter metadata
- generic instance keys
- generic substitution views
- generic parameter scopes
- generic unification helpers

### `nominal.rs`

- nominal declaration registration
- struct/choice definition builders
- receiver-key derivation over `TypeId`
- nominal generic instance views

### `queries.rs`

- semantic fact queries
- structural equality support query
- collection/result/option/function helper queries
- type class helpers

### `display.rs`

- type display through `StringTable`
- diagnostic-oriented rendering helpers
- no diagnostic policy

## Phase 0 — Preflight audit and migration map

### Summary

Before writing new type infrastructure, map every current `DataType` owner and consumer. The current code mixes type syntax, unresolved placeholders, semantic identity, nominal payloads, generic substitution, display, equality, structural equality support, and tests. This phase prevents accidentally building a parallel system beside it.

### Tasks

1. Search for all `DataType` usage.
2. Group usages by category:
   - parsed annotations
   - expression types
   - function signatures
   - declarations
   - constants
   - templates
   - type coercion
   - generic substitution
   - receiver catalog
   - structural equality
   - HIR lowering
   - diagnostics
   - backend lowering
   - tests
3. Search for all HIR type usage:
   - `hir_datatypes::TypeId`
   - `TypeContext`
   - `HirType`
   - `HirTypeKind`
   - HIR struct/choice registries
4. Search for duplicate type facts:
   - collection element lookup
   - result slot lookup
   - option inner lookup
   - numeric checks
   - structural equality support
   - receiver-key construction
   - nominal path extraction
   - generic instance display
5. Produce a migration table:
   - old type/function/module
   - current responsibility
   - new owner
   - migration phase
   - removal phase
6. Identify tests that are implementation-shaped around `DataType` and should be rewritten rather than preserved.

### Expected output

A short audit note committed before implementation begins, or included at the top of the first implementation PR/commit message.

### Audit checklist

- No implementation changes yet unless they are tiny cleanup.
- No new type system module yet.
- No compatibility wrappers introduced.
- No stale `DataType` behavior hidden behind new names.

### Validation

```bash
just validate
```

## Phase 1 — Create `datatypes/` module and core IDs

### Summary

Create the new type-environment skeleton without migrating all compiler users yet. This phase establishes the final ownership shape and prevents new work from extending `DataType`.

### Tasks

1. Convert `src/compiler_frontend/datatypes.rs` into `src/compiler_frontend/datatypes/mod.rs`.
2. Add:
   - `ids.rs`
   - `environment.rs`
   - `definitions.rs`
   - `parsed.rs`
   - `display.rs`
   - `queries.rs`
   - `nominal.rs`
   - updated `generics.rs`
3. Define:
   - `TypeId`
   - `NominalTypeId`
   - `GenericParameterId`
   - `GenericParameterListId`
   - `TypeKey`
   - `ConstructedTypeKey`
   - `GenericInstanceKey`
4. Add `TypeEnvironment`.
5. Seed builtin types.
6. Add canonical interning APIs:
   - `intern_builtin`
   - `intern_constructed`
   - `intern_function`
   - `register_nominal_struct`
   - `register_nominal_choice`
   - `intern_generic_instance`
7. Add basic queries:
   - `get`
   - `type_kind`
   - `is_numeric`
   - `is_collection`
   - `collection_element_type`
   - `result_slots`
   - `option_inner_type`
8. Add type display helpers:
   - `display_type`
   - `display_type_for_diagnostic`
9. Keep old `DataType` compiling only as a temporary migration artifact.
10. Add file-level module docs explaining:
    - `TypeEnvironment` owns semantic type identity
    - parsed type refs are not semantic type identity
    - backend layout does not belong here

### Expected output

The new module exists and can be unit-tested in isolation. Existing compiler behavior should remain unchanged.

### Tests

Add unit tests for:

- builtin seeding
- `TypeId` identity equality
- constructed type interning reuses existing IDs
- `{Int}` and `{String}` intern separately
- `Result<Int, Error>` interning is deterministic
- generic instance key equality
- display through `StringTable`

### Audit checklist

- `mod.rs` acts as a structural map, not a dumping ground.
- No broad `utils` module.
- No old API forwarding layer beyond temporary compile survival.
- All temporary compatibility is marked with removal phase comments.
- No backend layout concepts added.

### Validation

```bash
just validate
just bench-quick
```

## Phase 2 — Move parsed type syntax out of semantic type identity

### Summary

Separate unresolved parsed type syntax from resolved type identity. This removes the need for semantic types like `NamedType` and `Inferred`.

### Tasks

1. Add or migrate parsed annotation structures to `datatypes/parsed.rs`.
2. Replace header/declaration parsed type fields that currently use unresolved `DataType` variants with `ParsedTypeRef` or equivalent.
3. Remove semantic reliance on:
   - `DataType::NamedType`
   - `DataType::Inferred`
4. Add AST type-resolution APIs:
   - `resolve_type_ref(parsed, scope_context, type_environment) -> TypeId`
   - `resolve_type_annotation(parsed, ...) -> TypeAnnotationResolution`
5. Preserve source spelling in parsed refs or `WrittenType`, not in expression nodes.
6. Ensure type aliases resolve transparently to canonical `TypeId`.
7. Ensure unresolved names produce structured existing diagnostics at the same stage as before.
8. Do not change language syntax or visibility rules.

### Expected output

Unresolved type names are no longer represented as semantic types.

### Tests

Add/update tests for:

- unknown type name diagnostics
- alias-to-builtin resolution
- alias-to-struct resolution
- imported type alias resolution
- generic parameter name resolution
- inferred declaration types remain a checking state, not a table type

### Audit checklist

- No `TypeId` assigned to unresolved names.
- No expression carries written alias spelling.
- No duplicate name-resolution path added.
- Existing `ScopeContext` visibility rules stay authoritative.

### Validation

```bash
just validate
just bench-quick
```

## Phase 3 — Register nominal definitions in `TypeEnvironment`

### Summary

Move struct and choice definitions out of cloned `DataType` payloads and into canonical nominal definition tables.

### Tasks

1. Add nominal definition storage:
   - `struct_definitions`
   - `choice_definitions`
   - path/key maps
2. Register structs as nominal definitions during AST environment building.
3. Register choices as nominal definitions during AST environment building.
4. Replace `DataType::Struct { fields, ... }` usage with `TypeId` pointing to a nominal definition.
5. Replace `DataType::Choices { variants, ... }` usage with `TypeId` pointing to a nominal definition.
6. Add queries:
   - `struct_fields(type_id)`
   - `choice_variants(type_id)`
   - `nominal_path(type_id)`
   - `is_const_record(type_id)`
7. Move receiver-key derivation to use `TypeId + TypeEnvironment`.
8. Keep same language behavior:
   - structs remain nominal
   - choices remain nominal
   - aliases remain transparent
   - const records remain distinct where currently required
9. Remove duplicated nominal payload clones from AST declarations where practical.

### Expected output

Struct and choice identity is table-backed. Field/variant access goes through `TypeEnvironment`.

### Tests

Add/update tests for:

- same-shape structs are not interchangeable
- same nominal struct resolves to same `TypeId`
- same nominal choice resolves to same `TypeId`
- type aliases to structs are transparent
- const records preserve current behavior
- receiver method lookup still works
- choice constructor lookup still works
- pattern matching over choices still works

### Audit checklist

- No field/variant vectors cloned into every type occurrence.
- Receiver catalog does not reconstruct nominal identity manually.
- Structural equality support does not use `PartialEq` over old payloads.
- `DataType::Struct` and `DataType::Choices` are either gone or marked for immediate removal in the next phase.

### Validation

```bash
just validate
just bench-quick
```

## Phase 4 — Intern constructed builtin and function types

### Summary

Move collections, options, results, tuples/returns, and function types into canonical constructed/function type interning.

### Tasks

1. Add constructed type definitions for:
   - collection
   - option
   - result
   - tuple/multiple returns if needed
2. Add function type definitions for:
   - receiver
   - parameters
   - returns
3. Replace recursive payloads:
   - `DataType::GenericInstance` for collection
   - `DataType::Option`
   - `DataType::Result`
   - `DataType::Returns`
   - `DataType::Function`
4. Add queries:
   - `collection_element_type`
   - `option_inner_type`
   - `result_slots`
   - `function_signature`
   - `tuple_fields`
5. Update AST expression construction to use interned constructed types.
6. Update function signature resolution to store `TypeId`s directly.
7. Update builtin collection operations to use type queries, not pattern matching on recursive type payloads.
8. Keep explicit mutability and value-mode behavior separate from type identity.

### Expected output

Common constructed types are canonicalized and compact. Repeated `{Int}` or `Result<Int, Error>` references share the same `TypeId`.

### Tests

Add/update tests for:

- collection element type lookup
- empty collection explicit type requirement
- collection method type checking
- option assignment/coercion behavior
- result-returning calls
- multi-return signatures
- receiver function types
- function type equality by canonical IDs

### Audit checklist

- No collection/result/option recursive `DataType` tree remains in active semantic paths.
- No value-mode/access-mode state moved into type definitions.
- Type compatibility still lives in `type_coercion`.

### Validation

```bash
just validate
just bench-quick
```

## Phase 5 — Migrate generic parameter and generic instance handling

### Summary

Replace cloned generic substitution with canonical generic instance interning and lazy definition views.

### Tasks

1. Move generic parameter metadata to ID-backed tables.
2. Replace `GenericNominalInstantiationCache` from key-to-`DataType` with key-to-`TypeId`.
3. Define canonical generic instance storage:
   - base nominal ID
   - concrete argument type IDs
   - optional source/debug metadata
4. Replace substitution APIs that return cloned `DataType` with APIs that return:
   - `TypeId`
   - field views
   - variant views
   - signature views
5. Add lazy substitution views:
   - `fields_for(instance_type_id)`
   - `variants_for(instance_type_id)`
   - `constructor_signature_for(instance_type_id)`
6. Ensure generic instance keys use canonical `TypeId` arguments.
7. Ensure generic instance display uses canonical argument names through `TypeEnvironment + StringTable`.
8. Remove generic-instance payloads from nominal definitions.
9. Preserve current generics behavior implemented so far.
10. Keep deliberately deferred generic features deferred.

### Expected output

Repeated nominal generic instantiations share one canonical representation and no longer clone full struct/choice definitions eagerly.

### Tests

Add/update tests for:

- repeated `Box of Int` resolves to the same `TypeId`
- `Box of Int` and `Box of String` resolve to different `TypeId`s
- generic struct field lookup substitutes arguments correctly
- generic choice variant lookup substitutes arguments correctly
- generic constructor inference still works
- generic diagnostics still point to the source location of invalid application
- generic parameter collisions still fail

### Audit checklist

- No generic cache stores cloned type payloads.
- No eager substituted nominal definition is created unless memoized behind a query.
- Generic display does not bypass `TypeEnvironment`.
- No duplicate generic unification path remains.

### Validation

```bash
just validate
just bench-quick
```

## Phase 6 — Migrate AST nodes and AST environment APIs to `TypeId`

### Summary

Make AST the main producer of resolved `TypeId`s. AST should stop emitting cloned semantic type payloads.

### Tasks

1. Update expression structs to carry `TypeId`.
2. Update declarations to carry `TypeId` in resolved value/type slots.
3. Update constant folding outputs to carry `TypeId`.
4. Update template runtime expression typing to use `TypeId`.
5. Update call validation to compare/query `TypeId`s.
6. Update return validation to compare/query `TypeId`s.
7. Update match validation and exhaustiveness checks to query `TypeEnvironment`.
8. Update structural equality checks to call:
   - `type_environment.supports_runtime_equality(type_id)`
9. Update receiver catalog construction to use:
   - receiver `TypeId`
   - `TypeEnvironment` receiver-key query
10. Remove any AST pass/helper that exists only to patch around cloned `DataType` payloads.
11. Keep body-local declaration ordering and scope rules unchanged.

### Expected output

AST nodes and AST environment APIs use canonical type IDs for resolved semantic types.

### Tests

Add/update tests for:

- numeric operation typing
- function calls
- receiver calls
- struct construction
- choice construction
- pattern matching
- return validation
- constants
- template string coercion
- type mismatch diagnostics remain acceptable under the temporary diagnostic bridge

### Audit checklist

- No expression node carries a full semantic `DataType`.
- No AST helper reconstructs type identity from nominal path manually.
- Type rendering is confined to diagnostics/display helpers.
- Long argument lists introduced during migration are replaced with context structs.

### Validation

```bash
just validate
just bench-quick
```

## Phase 7 — Migrate `type_coercion` to `TypeId + TypeEnvironment`

### Summary

Compatibility stays policy-owned by `type_coercion`, but it must stop matching over `DataType` trees.

### Tasks

1. Update compatibility API:

```rust
pub fn is_assignable(
    actual: TypeId,
    expected: TypeId,
    type_environment: &TypeEnvironment,
    context: TypeCoercionContext,
) -> bool
```

2. Update numeric promotion APIs to use `TypeId`.
3. Update template/string boundary coercion to use `TypeId`.
4. Update declaration and return boundary coercion callers.
5. Update builtin cast checking.
6. Remove old `DataType` compatibility helpers.
7. Add focused tests for compatibility policy.

### Expected output

All contextual type compatibility decisions are made over `TypeId + TypeEnvironment`.

### Tests

Add/update tests for:

- exact assignability
- alias transparency
- Int/Float numeric promotion behavior
- no implicit Float-to-Int coercion
- template/string boundary behavior
- option wrapping behavior
- result/error handling behavior
- invalid collection element assignment

### Audit checklist

- `TypeEnvironment` does not absorb compatibility policy.
- Type coercion does not deep-compare semantic definitions except through environment queries.
- Numeric logic is not duplicated in AST expression evaluation.

### Validation

```bash
just validate
just bench-quick
```

## Phase 8 — Replace HIR semantic `TypeContext`

### Summary

Remove the duplicate HIR semantic type interner and make HIR consume frontend `TypeId`s.

### Tasks

1. Replace `compiler_frontend::hir::hir_datatypes::TypeId` with `compiler_frontend::datatypes::TypeId`.
2. Remove or rewrite `TypeContext`.
3. Remove `HirType` and `HirTypeKind` where they duplicate frontend semantic types.
4. Replace HIR type construction during AST-to-HIR lowering with direct `TypeId` threading.
5. Replace HIR type classification with helper functions over `TypeEnvironment`.
6. Remove HIR comments that suggest layout/drop/ABI metadata belongs in semantic type identity.
7. Update HIR display/debug code to receive `TypeEnvironment`.
8. Update HIR side tables if they store HIR-specific type IDs.
9. Update borrow checker inputs to use semantic `TypeId`.
10. Update backend lowering contexts to receive `TypeEnvironment`.

### Expected output

There is one semantic type identity system. HIR carries `TypeId`s and the outer compiled module carries `TypeEnvironment`.

### Tests

Add/update tests for:

- HIR generation for primitives
- HIR generation for structs
- HIR generation for choices
- HIR generation for collections
- HIR generation for functions
- borrow checker still validates mutable/shared access
- HIR debug output still renders useful type names

### Audit checklist

- No `hir_datatypes::TypeId` remains.
- No semantic HIR `TypeContext` remains.
- No backend layout metadata is moved into `TypeEnvironment`.
- HIR remains backend-facing semantic IR, not layout IR.

### Validation

```bash
just validate
just bench-quick
```

## Phase 9 — Move HIR struct/choice registries into `TypeEnvironment`

### Summary

Remove duplicate nominal registries from HIR once backends and borrow validation can query nominal definitions through `TypeEnvironment`.

### Tasks

1. Remove `HirModule.structs`.
2. Remove `HirModule.choices`.
3. Move any HIR-only nominal metadata still needed by backends into `TypeEnvironment` definitions or explicit lowering metadata.
4. Update choice variant indexes to be available through `TypeEnvironment`.
5. Update struct field lookup in backends to use `TypeEnvironment`.
6. Update pattern/match lowering if it consumes HIR choice registries.
7. Update Wasm HIR-to-LIR lowering context to query `TypeEnvironment`.
8. Update JS/backend builder paths if they consume HIR nominal registries.
9. Keep backend-specific layout decisions separate.

### Expected output

Nominal metadata has one owner: `TypeEnvironment`.

### Tests

Add/update tests for:

- struct field lowering
- choice variant lowering
- choice payload lowering
- pattern match lowering
- backend artifact assertions that previously depended on HIR registries

### Audit checklist

- No copied nominal tables remain in HIR.
- Backends do not mutate `TypeEnvironment`.
- Backends do not attach layout to frontend definitions.
- Any target-specific derived table has a clear backend owner.

### Validation

```bash
just validate
just bench-quick
```

## Phase 10 — Update compiled module/frontend output boundary

### Summary

Move `TypeEnvironment` to the outer compiled module boundary so all later stages receive semantic type identity explicitly.

### Tasks

1. Identify the current frontend output type consumed by builders.
2. Add `type_environment: TypeEnvironment` to the outer module payload.
3. Remove type environment ownership from HIR if it was placed there temporarily.
4. Update frontend pipeline return types.
5. Update borrow validation call sites.
6. Update backend builder inputs.
7. Update dev server/build-system boundaries if they clone/remap modules.
8. Ensure string ID remapping includes any string IDs inside `TypeEnvironment`.
9. Ensure diagnostics and warnings still carry/remap source locations correctly.
10. Ensure module constants and page fragments still have access to type display if needed.

### Expected output

Backends receive:

- HIR
- `TypeEnvironment`
- borrow facts
- warnings
- constants
- entry metadata
- page fragments

without HIR owning duplicate type tables.

### Tests

Add/update tests for:

- full frontend compile output
- docs build
- dev/check/build modes if they use different frontend paths
- string ID remapping across module outputs
- diagnostic rendering after remapping

### Audit checklist

- `TypeEnvironment` ownership is explicit.
- No stage reaches backward into AST internals to find types.
- No type table is cloned unnecessarily across every function/node.
- No hidden global type environment is introduced.

### Validation

```bash
just validate
just bench-quick
```

## Phase 11 — Remove old `DataType` semantic paths

### Summary

Delete the old system once all active semantic users have migrated.

### Tasks

1. Delete old `DataType` variants that represented resolved semantic types.
2. Delete old `PartialEq for DataType` semantic equality logic.
3. Delete old `display_with_table` paths that are replaced by `TypeEnvironment`.
4. Delete old structural equality helpers on `DataType`.
5. Delete old generic substitution returning cloned `DataType`.
6. Delete old collection/option/result helper methods.
7. Delete obsolete tests that assert old implementation details.
8. Rewrite tests that should assert behavior through the new environment.
9. Remove `#[allow(dead_code)]` annotations that existed only for old type variants.
10. Remove stale comments that describe old ownership.

### Expected output

There is no active semantic `DataType` system. The compiler has one resolved type identity path.

### Tests

Add/update tests for:

- same behavior covered before migration
- old implementation-shaped tests replaced with behavior-focused tests
- type environment unit tests cover canonicalization
- integration tests cover language-facing behavior

### Audit checklist

- No compatibility wrapper remains.
- No parallel type representation remains.
- No stale comments imply old behavior.
- No old tests force bad structure to remain.
- No new broad abstractions were added to hide cleanup.

### Validation

```bash
just validate
just bench-quick
```

## Phase 12 — Documentation and progress matrix updates

### Summary

Update documents affected by the implementation. This phase should be a separate commit after code behavior is stable.

### Tasks

1. Update `docs/compiler-design-overview.md`:
   - `datatypes/` owns `TypeEnvironment`
   - AST resolves parsed type syntax into `TypeId`
   - HIR carries semantic `TypeId`s
   - compiled modules carry `TypeEnvironment`
   - HIR no longer has a separate semantic `TypeContext`
   - backend layout remains backend-owned
2. Update `docs/src/docs/progress/#page.bst`:
   - mark frontend type identity redesign as implemented or in-progress
   - mark generic nominal instance interning status
   - note structured type diagnostics as deferred follow-up
3. Update any roadmap/matrix entries that become stale due to discovered implementation gaps.
4. Do not update language semantics docs unless behavior changed.
5. Do not modify generated docs release artifacts directly.

### Expected output

Docs match implemented stage ownership and do not imply duplicated type interners.

### Audit checklist

- Documentation describes stage ownership, not transient implementation churn.
- No language behavior changes are implied.
- Deferred diagnostics work is explicitly marked as future work.
- Progress matrix is not left stale.

### Validation

```bash
just validate
```

## Phase 13 — Final cleanup, benchmark pass, and implementation report

### Summary

Perform a final quality pass to ensure the refactor produced less duplication and clearer ownership, not just a new layer on top of old code.

### Tasks

1. Search for old symbols:
   - `DataType`
   - `TypeContext`
   - `HirTypeKind`
   - `GenericNominalInstantiationCache`
   - `data_type_to_type_identity_key`
   - `display_with_table`
2. Confirm any remaining usages are:
   - parsed syntax only
   - test-only
   - deliberately deferred with a clear comment
3. Search for duplicate type helper logic:
   - numeric checks
   - collection element lookup
   - result slots
   - option inner
   - nominal path lookup
   - structural equality support
   - receiver type keying
4. Run benchmark comparison against baseline.
5. Write a short implementation report:
   - what changed
   - deleted old paths
   - deferred follow-ups
   - benchmark observations
   - diagnostic limitations left for future structured diagnostic refactor

### Expected output

The compiler has one canonical semantic type identity system. The implementation report gives future agents a clear handoff.

### Validation

```bash
just validate
just bench-ci
```

## Final acceptance criteria

- `TypeId` is the canonical resolved semantic type identity.
- `TypeId` equality is the default semantic type equality check.
- Type identity lookup is table-backed and deterministic.
- Builtin types are seeded in `TypeEnvironment`.
- Nominal struct and choice definitions live in `TypeEnvironment`.
- Generic nominal instantiations are interned and reusable.
- Constructed builtin types are interned and reusable.
- Type aliases resolve transparently to canonical targets.
- Parsed/unresolved type syntax is not represented as semantic `TypeId`.
- Inference placeholders are not semantic types.
- AST nodes carry `TypeId`s for resolved expression/declaration types.
- Function signatures store `TypeId`s directly.
- `type_coercion` operates on `TypeId + TypeEnvironment`.
- HIR does not own a separate semantic type interner.
- HIR struct/choice registries are removed or reduced to non-duplicating lowering metadata.
- The compiled module boundary carries `TypeEnvironment`.
- Backend layout/ABI/drop/runtime metadata is not stored in `TypeEnvironment`.
- Diagnostics remain functional with temporary type rendering/plumbing.
- Structured `TypeId` diagnostics are explicitly deferred to the later diagnostic refactor.
- Old `DataType` semantic paths are deleted.
- No compatibility wrappers preserve obsolete type APIs.
- Tests cover behavior rather than old implementation accidents.
- Docs and progress matrix are updated after implementation.

## Deliberately deferred follow-ups

### Structured diagnostics

Move diagnostics away from eager strings and toward structured diagnostic enums carrying `TypeId`, `SourceLocation`, and context enums.

Example:

```rust
pub enum Diagnostic {
    TypeMismatch {
        location: SourceLocation,
        expected: TypeId,
        found: TypeId,
        context: TypeMismatchContext,
    },
}
```

This should be a separate plan after the type environment redesign.

### Backend layout environments

Introduce backend-specific layout/type environments only when LIR/backend lowering needs them.

Potential future shape:

```rust
pub struct LayoutEnvironment {
    pub layouts_by_type: FxHashMap<TypeId, LayoutTypeId>,
}
```

Do not prebuild this in the frontend.

### Cross-module/package-stable type identity

Module-local `TypeId`s are enough for this redesign.

Package caching, source-library HIR caching, and cross-build stable type identity should use stable keys/fingerprints later, not raw `TypeId`.

### External generic type metadata

External generic types should remain deferred until external package metadata can describe generic type constructors cleanly.

### Full monomorphization strategy

This plan interns semantic generic instances. It does not decide final backend monomorphization, ABI specialization, or code duplication strategy.

## Implementation notes for agents

- Read `AGENTS.md`, `docs/codebase-style-guide.md`, `docs/compiler-design-overview.md`, `docs/language-overview.md`, and `docs/memory-management-design.md` before implementation.
- Before adding a new helper or module, search for existing overlapping logic.
- Prefer updating, generalizing, or deleting existing owners over creating parallel paths.
- Do not preserve obsolete APIs through wrappers.
- Prefer context structs over long argument lists.
- Keep `mod.rs` files as structural maps.
- Add concise WHAT/WHY comments for non-obvious type identity, interning, and stage-boundary logic.
- Avoid clever iterator-heavy Rust in migration code.
- Keep all user-input failures diagnostic-based, not panic-based.
- Run `just validate` before each phase is considered complete.
- Run `just bench-quick` after major type-flow migrations.
- Run `just bench-ci` at the end.
