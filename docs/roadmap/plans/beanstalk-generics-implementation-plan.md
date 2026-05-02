# Beanstalk Generics Implementation Plan

This plan describes how to add generics to Beanstalk in a way that fits the current compiler architecture, avoids duplicate type-system paths, and keeps the implementation maintainable.

It is anchored in:

- `docs/src/docs/generics/#page.bst`
- `docs/src/docs/progress/#page.bst`
- `docs/compiler-design-overview.md`
- `docs/codebase-style-guide.md`
- the current frontend/HIR structure in `src/compiler_frontend/`

## Core direction

Generics are a frontend type-system feature.

The AST/type-resolution stage must resolve generic declarations, generic type uses, generic function calls, generic struct construction, and generic choice construction before HIR lowering. HIR should not receive unresolved type parameters in executable code. HIR may later store generic declaration metadata if useful, but executable HIR must remain concrete.

This follows the existing compiler contract: AST owns type checking and name/type resolution, while HIR receives fully typed semantic input for control-flow lowering and borrow validation.

## Pre-release implementation policy

Beanstalk is pre-release. This plan deliberately does **not** preserve old internal APIs, compatibility wrappers, legacy `DataType` shapes, or duplicate paths for old/new type handling.

When a new generic-ready model replaces an older model, the old model should be removed cleanly once the replacement is live.

Do not add:

- compatibility wrappers
- parallel generic-specific type parsers
- string-based generic identities
- temporary generic/result/trait hacks
- alternate old/new collection representations after migration
- backend-specific generic solving

The right outcome is one current compiler shape, even if it requires touching more call sites.

## Confirmed language decisions

| Area | Decision |
|---|---|
| Generic solving stage | Before HIR, likely permanently. |
| Generic nominal identity | `Box of Int` and `Box of String` are distinct nominal instantiations. |
| Collection internals | Collections migrate onto generic machinery, while preserving `{T}` syntax. |
| Collection public syntax | Only `{T}` / `{T capacity}` for collections, not public `Collection of T`. |
| Collection capacity | `{Int 64}` is allocation metadata, not part of type identity. |
| Option/Error | Keep distinct semantic forms. Do not force them into ordinary user generics. |
| Type aliases | Support aliases to fully concrete generic instances. Defer/avoid generic aliases. |
| Type-constructor composition | Defer. Ignore forms like `BoxedMap as Box of Map` for now. |
| Generic struct constructors | Use normal constructor name and infer from immediate constructor args plus expected type. |
| Generic choice constructors | Infer from payload args plus immediate expected type. Reject unknown type parameters. |
| `of` expression use | `of` is type-position only in alpha. Explicit generic call syntax is deferred. |
| Generic parameter collisions | Reject collisions with visible types, aliases, builtins, and external type names. |
| Generic parameter names | PascalCase or single uppercase only. Hard error on violations. |
| Unused generic params | Hard error. Generic params must appear in the declaration type shape. |
| Recursive generic types | Deferred. Reject cleanly. |
| Generic receiver methods | Deferred. User-authored generic receiver methods may never be medium-term support. |
| Receiver methods on generic instances | Deferred too, even concrete `this Box of Int`. Use free functions. |
| Import/export | Use existing facade/import model. Generic declaration exports, not specific instantiations. |
| Nested `of` | Reject more than one `of` application in a single type annotation. Collection element exception allowed. |
| Generic function inference | Use immediate call arguments and immediate expected declaration/return context only. Never infer from later uses. |
| Unconstrained `T` operations | Only type-preserving structural movement before traits. Behavior-dependent operations wait for traits. |
| Generic function calls from generic functions | Allowed only under the same immediate inference rules. |
| Generic function instantiation cache | Yes, frontend cache keyed by canonical function path + concrete type args. |

## Alpha generic scope

### Include

- `type` generic parameter declaration syntax.
- `of` generic type application syntax in type positions.
- Generic structs.
- Generic choices.
- Generic free functions with constrained alpha body rules.
- Generic function inference from immediate arguments and immediate expected type context.
- Generic struct/choice constructor inference from immediate constructor payload plus expected type context.
- Aliases to fully concrete generic instances.
- Collection migration onto generic type infrastructure.
- Collection capacity syntax: `{Int 64}`.
- Import/export/re-export support through existing module facade rules.
- Targeted diagnostics for all deferred generic surfaces.

### Defer

- Trait bounds.
- Any behavior-dependent operation on unconstrained generic parameters.
- Explicit generic call-site application, such as `identity of Int(42)`.
- Generic receiver methods.
- Receiver methods on generic instantiated types.
- Recursive generic structs/choices.
- Generic type aliases.
- Partial type application / type-constructor composition.
- Nested generic type applications beyond one `of` per annotation.
- Higher-kinded types.
- Const generics.
- Lifetime parameters.
- Specialization.
- Associated types / generic associated types.
- Advanced inference.
- Dynamic generic reification.

## Key architectural rule

Generics must be represented structurally, not as strings.

Bad shapes:

```rust
DataType::GenericStringName("Box of Int".to_owned())
HashMap<String, DataType>
```

Correct direction:

```rust
DataType::GenericInstance {
    base: GenericBaseType,
    arguments: Vec<DataType>,
}
```

The display layer may render `Box of Int`, but identity and substitution must use structured IDs/keys.

---

# Phase 0 — Generic-ready type-system substrate

## Purpose

Phase 0 does **not** implement user-visible generics.

It prepares the compiler type infrastructure for generics, collections-on-generic-machinery, options/results hardening, and traits without adding temporary feature-specific hacks.

Target state:

```text
One canonical type model.
One canonical generic metadata model.
One canonical type-resolution path.
No parallel generic/result/trait special cases.
```

## Context

Current relevant compiler state:

- `DataType` is the frontend semantic type model and already carries `NamedType`, `Collection`, `Struct`, `Choices`, `Option`, `Result`, `Function`, and external types.
- `declaration_syntax/type_syntax.rs` centralizes type annotation parsing and recursive named-type resolution.
- Header declarations carry concrete shapes for functions, structs, choices, constants, and aliases, but not generic parameter metadata.
- `ModuleSymbols` owns top-level symbol metadata, imports, facades, type aliases, builtins, and declaration placeholders, but not generic declaration metadata.
- AST already has a useful sequence: import bindings, type alias resolution, type/signature resolution, receiver catalog, body emission, finalization.
- HIR already has canonical `TypeId` storage and concrete type kinds for collections, structs, choices, options, results, functions, and external types.

## Step 0.1 — Add compiler design note

Add:

```text
docs/compiler-generic-type-model.md
```

It should define:

```rust
TypeParameter
TypeArgument
GenericParameterList
GenericDeclaration
GenericInstantiation
TypeSubstitution
GenericInstantiationKey
TypeIdentityKey
```

It must state:

```text
Phase 0 does not implement user-visible generics.
Phase 0 only adds compiler infrastructure for generic declarations and generic type references.
```

It must lock these decisions:

| Question | Decision |
|---|---|
| Are generic parameters types only? | Yes. No const generics. No lifetime generics. |
| Are trait bounds part of Phase 0? | No. Leave space for traits, but do not add fake bounds infrastructure. |
| Are generics nominal or structural? | Generic structs/choices remain nominal after instantiation. |
| Are generic aliases transparent? | Concrete generic aliases are transparent, same as current aliases. Generic aliases are deferred. |
| Are `Option` / error results ordinary generic choices? | No. They keep distinct semantics. Shared lowering mechanisms are fine; shared semantics are not required. |
| Are generic functions monomorphized immediately? | Do not encode backend strategy in the type model. AST resolves concrete executable uses before HIR. |
| Are collections ordinary public generics? | No. `{T}` remains the public collection type syntax, but collection internals migrate to generic type infrastructure. |
| Is collection capacity a type argument? | No. Capacity is allocation metadata. |

## Step 0.2 — Add generic data structs

Create a new focused module, preferably:

```text
src/compiler_frontend/types/generics.rs
```

If a larger `types/` module move is too invasive for the first commit, use:

```text
src/compiler_frontend/datatypes/generics.rs
```

But avoid leaving the compiler permanently split between `datatypes.rs` and unrelated generic files. The end-state should make the type model easy to navigate.

Initial structs:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeParameterId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericParameter {
    pub id: TypeParameterId,
    pub name: StringId,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GenericParameterList {
    pub parameters: Vec<GenericParameter>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeArgumentList {
    pub arguments: Vec<DataType>,
}
```

Do **not** add trait bounds yet unless they are immediately used. Empty “future” fields create noise and dead paths. Traits are coming soon, but Phase 0 should not fake trait solving.

## Step 0.3 — Add generic parameter scopes

Add:

```rust
pub(crate) struct GenericParameterScope {
    parameters_by_name: FxHashMap<StringId, GenericParameter>,
}
```

Helper methods:

```rust
impl GenericParameterScope {
    pub(crate) fn empty() -> Self;
    pub(crate) fn from_parameter_list(...) -> Result<Self, CompilerError>;
    pub(crate) fn resolve(&self, name: StringId) -> Option<&GenericParameter>;
    pub(crate) fn contains_name(&self, name: StringId) -> bool;
}
```

Eventually enforced rules:

- Duplicate generic parameter names are errors.
- Generic parameter names cannot collide with visible type names, type aliases, builtins, or external types.
- Generic parameter names must be PascalCase or single uppercase.
- Generic parameter scope is declaration-local.
- Generic parameters do not become value symbols.
- Generic parameters are not importable declarations.
- Generic parameters cannot be used outside the generic declaration body/signature.

Phase 0 adds the object and tests but keeps all live scopes empty.

## Step 0.4 — Extend `DataType`

Add generic-ready frontend variants:

```rust
pub enum DataType {
    // Existing variants...

    TypeParameter {
        id: TypeParameterId,
        name: StringId,
    },

    GenericInstance {
        base: GenericBaseType,
        arguments: Vec<DataType>,
    },

    // Existing variants...
}
```

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GenericBaseType {
    Named(StringId),
    ResolvedNominal(InternedPath),
    External(ExternalTypeId),
    Builtin(BuiltinGenericType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinGenericType {
    Collection,
}
```

Do **not** remove `DataType::Collection` in Phase 0. Phase 2 owns the live migration and clean deletion.

Update all core helpers:

- `PartialEq`
- `display_with_table`
- `supports_structural_equality`
- recursive traversal helpers
- named-type collection helpers
- any debug/HIR display helpers that pattern match on `DataType`

Display using Beanstalk syntax:

```text
T
Box of Int
Pair of String, Int
{Box of String}
Box of Int?
```

Do not display user diagnostics with angle brackets.

## Step 0.5 — Add type identity keys

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenericInstantiationKey {
    pub base_path: InternedPath,
    pub arguments: Vec<TypeIdentityKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeIdentityKey {
    Builtin(BuiltinTypeKey),
    Nominal(InternedPath),
    External(ExternalTypeId),
    Collection(Box<TypeIdentityKey>),
    Option(Box<TypeIdentityKey>),
    Result {
        ok: Box<TypeIdentityKey>,
        err: Box<TypeIdentityKey>,
    },
    GenericInstance(GenericInstantiationKey),
}
```

Purpose:

- Avoid stringified fake type names.
- Avoid anonymous structural generic instances.
- Give generic instantiation caches stable keys.
- Prepare HIR/backend identity without forcing generic HIR.

Phase 0 tests equality/display only. Do not wire everywhere until needed.

## Step 0.6 — Add type substitution infrastructure

Add:

```rust
pub(crate) struct TypeSubstitution {
    replacements: FxHashMap<TypeParameterId, DataType>,
}

pub(crate) fn substitute_type_parameters(
    data_type: &DataType,
    substitution: &TypeSubstitution,
) -> DataType
```

It must recursively support:

- `TypeParameter`
- `GenericInstance`
- `Collection`
- `Option`
- `Result`
- `Reference`
- `Returns`
- `Function`
- `Struct` fields
- `Choices` payload fields
- aliases after resolution

Keep this unused by the live pipeline in Phase 0. Add unit tests only.

## Step 0.7 — Refactor `type_syntax.rs` into layered parsing

Current `parse_type_annotation` is centralized. Keep it centralized, but split it into a cleaner grammar:

```text
parse_type_annotation
  parse_required_type
    parse_type_atom
    parse_type_postfixes
```

Target parse order:

```text
TypeAtom
GenericArguments?
OptionalSuffix?
```

Phase 0 adds `type` and `of` tokens to the tokenizer, but does not implement user-visible generic parsing yet.

Phase 0 rejection behavior:

- `of` outside any supported context: structured syntax error.
- `of` in type annotations before Phase 1: structured deferred-feature diagnostic.
- `type` after declaration names before Phase 1: structured deferred-feature diagnostic.

Because this is pre-release, reserving `type` and `of` now is acceptable. Do not preserve ability to use them as identifiers.

## Step 0.8 — Replace callback-heavy type resolution with `TypeResolutionContext`

Current named-type resolution is centralized but callback-based. Generics will make callback resolution brittle.

Introduce:

```rust
pub(crate) struct TypeResolutionContext<'a> {
    pub declarations: &'a [Declaration],
    pub visible_declaration_ids: Option<&'a FxHashSet<InternedPath>>,
    pub visible_external_symbols: Option<&'a FxHashMap<StringId, ExternalSymbolId>>,
    pub visible_source_bindings: Option<&'a FxHashMap<StringId, InternedPath>>,
    pub visible_type_aliases: Option<&'a FxHashMap<StringId, InternedPath>>,
    pub resolved_type_aliases: Option<&'a FxHashMap<InternedPath, DataType>>,
    pub generic_parameters: Option<&'a GenericParameterScope>,
}
```

Expose:

```rust
pub(crate) fn resolve_type(
    data_type: &DataType,
    location: &SourceLocation,
    context: &TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> Result<DataType, CompilerError>
```

Resolution order:

1. Local generic parameters.
2. Visible type aliases.
3. Visible source declarations.
4. External types.
5. Builtins.
6. Later: trait names in bound position only.

This is the highest-leverage cleanup in Phase 0. Without it, generics will be duplicated across signature resolution, alias resolution, struct fields, choice payloads, and call validation.

## Step 0.9 — Extend header contracts

Update `HeaderKind` shape even before parsing generic syntax:

```rust
pub enum HeaderKind {
    Function {
        generic_parameters: GenericParameterList,
        signature: FunctionSignature,
    },
    Constant {
        declaration: DeclarationSyntax,
    },
    Struct {
        generic_parameters: GenericParameterList,
        fields: Vec<Declaration>,
    },
    Choice {
        generic_parameters: GenericParameterList,
        variants: Vec<ChoiceVariant>,
    },
    TypeAlias {
        generic_parameters: GenericParameterList,
        target: DataType,
    },
    ConstTemplate,
    StartFunction,
}
```

Every parser fills `GenericParameterList::default()` in Phase 0.

This is intentional. It forces downstream passes to see the future shape before generics are implemented.

## Step 0.10 — Extend `ModuleSymbols`

Add:

```rust
pub(crate) generic_declarations_by_path:
    FxHashMap<InternedPath, GenericDeclarationMetadata>
```

With:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GenericDeclarationKind {
    Function,
    Struct,
    Choice,
    TypeAlias,
}

#[derive(Clone, Debug)]
pub(crate) struct GenericDeclarationMetadata {
    pub(crate) kind: GenericDeclarationKind,
    pub(crate) parameters: GenericParameterList,
    pub(crate) declaration_location: SourceLocation,
}
```

The map can remain empty until Phase 1, but the API and ownership should be in place.

## Step 0.11 — Preserve HIR concrete-type contract

Do not add unresolved generic type parameters to executable HIR.

Add or update comments near `hir_datatypes.rs`:

```text
HIR must never receive unresolved generic declarations or type parameters in executable bodies.
Generic declarations may exist as frontend metadata before instantiation, but lowered executable HIR uses concrete TypeIds.
```

Do not remove HIR’s concrete `Collection`, `Option`, or `Result` kinds in Phase 0.

## Step 0.12 — Preserve shared variant lowering direction

Document this rule:

```text
Choices, Options, and Results may share variant construction/payload carrier lowering.
They must not be forced to share all semantic rules.
```

Meaning:

- Choices have tag/exhaustiveness semantics.
- Options have `none` contextual rules.
- Results have propagation and error-channel rules.
- Backends may share carrier/layout machinery.

## Step 0.13 — Audit Option/Result paths

Add an audit note, either in `docs/compiler-generic-type-model.md` or a separate implementation note:

```text
docs/roadmap/audits/option-result-type-path-audit.md
```

Inventory:

- Option parsing.
- `none` expression parsing.
- Option compatibility/coercion.
- Result return slot parsing.
- `return!` parsing.
- `call(...)!` propagation.
- fallback syntax.
- named handler syntax.
- Result HIR expressions.
- JS backend result lowering.
- collection `get()` result typing.
- builtin `Error` optional fields.

Classify each path:

| Category | Action |
|---|---|
| Correct shared infrastructure | Keep. |
| Temporary syntax hack | Replace in result/option hardening. |
| Duplicate lowering | Consolidate. |
| Backend-only workaround | Isolate behind typed HIR operation. |
| Missing diagnostic | Add later task. |

## Step 0.14 — Phase 0 tests

Add unit tests. Do not add user-visible generic integration tests yet.

### Substitution tests

```text
T -> Int
{T} -> {Int}
T? -> Int?
Result<T, Error> -> Result<String, Error>
Pair<T, U> -> Pair<Int, String>
```

### Type identity tests

```text
Box of Int == Box of Int
Box of Int != Box of String
Pair of Int, String != Pair of String, Int
```

### Display tests

```text
T
Box of Int
Box of Int?
{Box of String}
Result of Box of Int, Error
```

### Resolver tests

- Generic parameter lookup works in generic scope.
- Generic parameter collisions are rejected by helper methods.
- Generic parameter names must be PascalCase or single uppercase.

### No-regression tests

Existing tests must still pass for:

- type aliases
- options
- results
- builtin error types
- choices
- receiver methods
- imports
- external types
- collections

## Phase 0 audit / style / validation commit

Audit checklist:

- No user-visible generics accepted yet.
- `type` and `of` are reserved and rejected cleanly where unsupported.
- No old/new generic API wrappers.
- Generic metadata types are documented.
- `DataType` additions are structurally represented, not stringified.
- Type resolution now has a context object.
- Header and module symbol shapes are generic-ready.
- HIR executable type contract remains concrete.
- All new modules have file-level docs.
- No user-input panics, `todo!`, or `.unwrap()` paths.

Validation:

```bash
just validate
```

Minimum fallback:

```bash
cargo clippy
cargo test
cargo run tests
```

Commit name:

```text
Prepare generic-ready type infrastructure
```

---

# Phase 1 — Parse generic declarations and type applications

## Purpose

Phase 1 makes generic syntax parse into the canonical generic metadata model. It should still avoid body/codegen complexity where possible.

## Supported syntax

```bst
identity type T |value T| -> T:
    return value
;

Box type T = |
    value T,
|

ResultShape type T, E ::
    Ok T,
    Err E,
;

StringBox as Box of String
```

## Step 1.1 — Tokenization

`type` and `of` should already be reserved from Phase 0. Ensure:

```rust
TokenKind::Type
TokenKind::Of
```

or equivalent names exist.

Update keyword diagnostics so `type` and `of` cannot be used as identifiers.

## Step 1.2 — Parse generic parameter lists

Add shared parser:

```rust
parse_generic_parameter_list_after_type_keyword(...)
```

It is used by:

- function headers
- struct headers
- choice headers
- type aliases only for rejected/deferred diagnostics initially

Rules:

- generic params start after `type`
- comma-separated
- PascalCase or single uppercase only
- no duplicates
- no collision with visible type names/builtins/external types/type aliases
- no empty list after `type`
- no bounds syntax yet
- no `where` yet

Examples:

```bst
identity type T |value T| -> T:
Box type T = | value T |
Pair type Left, Right = | left Left, right Right |
```

Rejected:

```bst
identity type |value Int| -> Int:
identity type t |value t| -> t:
identity type Int |value Int| -> Int:
identity type T, T |value T| -> T:
```

## Step 1.3 — Attach generic metadata to headers

Header parsers should fill:

```rust
generic_parameters: GenericParameterList
```

for functions, structs, and choices.

For type aliases:

- no generic aliases in alpha
- if `type` appears after alias name, reject with deferred diagnostic

Example rejected:

```bst
Response type T as Result of T, Error
```

Diagnostic direction:

```text
Generic type aliases are not supported yet.
Use an alias to a fully concrete generic type, such as `StringBox as Box of String`.
```

## Step 1.4 — Register generic declaration metadata

When a header has non-empty generic params, register:

```rust
GenericDeclarationMetadata
```

in `ModuleSymbols.generic_declarations_by_path`.

Use canonical declaration paths, not local aliases.

Imported generic declarations should resolve through existing import/facade maps.

## Step 1.5 — Parse generic type application with `of`

Extend `type_syntax.rs`:

```text
TypeAtom
GenericApplication?
OptionalSuffix?
```

Surface syntax:

```bst
Box of Int
Pair of String, Int
ResultShape of String, Error
StringBox as Box of String
```

Restrictions:

- `of` is valid only in type positions.
- exactly one `of` application per type annotation, except collection element type may itself be one generic application.
- generic arguments must be type atoms or collection types, not another inline `of` application beyond the allowed collection element exception.
- type application must match declaration arity.
- missing/extra args are hard errors.
- applying `of` to non-generic types is an error.
- omitting `of` on generic type in a concrete type position is an error unless inference context owns it.

Valid:

```bst
value Box of String = Box("x")
lookup Map of String, Int = create_map()
items {Box of String} = {}
items {Map of String, Int} = {}
```

Invalid:

```bst
value Box of Map of String, Int = ...
value ResultShape of Box of String, Error = ...
value Map of String, Box of Int = ...
value Box of Map = ...
```

Recommended diagnostic:

```text
Nested generic type applications are not supported in a single annotation.
Name the inner type with a concrete type alias first.
```

## Step 1.6 — Concrete generic aliases

Support aliases to fully concrete generic applications:

```bst
StringBox as Box of String
StringIntMap as Map of String, Int
BoxedStringIntMap as Box of StringIntMap
```

Reject partial/generic aliases:

```bst
BoxedMap as Box of Map
Response type T as ResultShape of T, Error
```

Alias resolution remains transparent. Do not create nominal alias identities.

## Step 1.7 — Resolve generic parameters in declaration type shapes

For each generic declaration:

- create a `GenericParameterScope`
- resolve parameter/field/variant payload type names against that scope first
- produce `DataType::TypeParameter` for matches
- then resolve normal visible types through `TypeResolutionContext`

Validate generic parameter usage:

- every declared generic parameter must appear in the declaration public type shape
- for functions: parameter types or return types
- for structs: fields
- for choices: payload fields
- body-only usage does not count

Rejected:

```bst
unused type T |value Int| -> Int:
    return value
;

Box type T = |
    value Int,
|
```

## Step 1.8 — Reject recursive generic types

Detect and reject recursive generic declarations.

Examples rejected:

```bst
Node type T = |
    value T,
    children {Node of T},
|

Tree type T ::
    Branch | children {Tree of T} |,
    Leaf T,
;
```

Use a clear deferred diagnostic:

```text
Recursive generic types are not supported yet.
Use a non-recursive shape or split the recursive storage behind a future indirection type.
```

## Step 1.9 — Import/export/facade behavior

Generic declarations use the same visibility rules as normal declarations.

```bst
# Box type T = |
    value T,
|

#import @./box/Box as Container
```

Rules:

- `#` exports the generic declaration.
- `#mod.bst` re-exports the generic declaration.
- import aliases rename the declaration locally.
- instantiation identity uses canonical resolved declaration path.
- aliases follow existing collision rules.

## Phase 1 audit / style / validation commit

Audit checklist:

- one generic parameter parser
- one generic type-application parser
- `of` type-position-only enforced
- no generic body lowering yet unless needed by parser tests
- no generic aliases
- no nested generic chaos
- no stringified identities
- imported generic declarations use canonical paths
- docs draft updated for all restrictions
- progress matrix updated

Tests:

- tokenizer keyword tests for `type` and `of`
- header parsing tests for generic functions/structs/choices
- type syntax tests for `of`
- negative diagnostics for collisions, bad names, duplicate params, unused params, wrong arity, non-generic `of`, missing args, nested `of`, generic aliases, recursive generics
- import/facade parser tests for generic declarations

Validation:

```bash
just validate
```

Commit name:

```text
Parse generic declarations and type applications
```

---

# Phase 2 — Migrate collections onto generic type infrastructure

## Purpose

Collections are the first live compiler-owned generic substrate migration.

The public syntax remains `{T}` and `{T capacity}`. Internally, collections should use the same generic type machinery as user generic instances where useful, while HIR keeps concrete `HirTypeKind::Collection`.

At the end of Phase 2, old `DataType::Collection(Box<DataType>)` should be removed completely.

## Step 2.1 — Add collection type annotation metadata

Capacity is not part of the type. It is allocation metadata.

Add a syntax/annotation metadata shape separate from `DataType`:

```rust
pub struct CollectionTypeAnnotation {
    pub element_type: DataType,
    pub initial_capacity: Option<i64>,
    pub capacity_location: Option<SourceLocation>,
}
```

Possible storage choices:

- attached to declaration syntax metadata
- attached to expression initializer metadata
- side table keyed by declaration path/local symbol

Do **not** model capacity as a generic argument.

## Step 2.2 — Parse `{T capacity}`

Rules:

```bst
collection {Int 64} = {}
```

- must contain a type first
- may contain exactly one integer capacity after the type
- capacity must be a positive integer literal, or zero if zero-capacity preallocation is meaningful
- no extra types
- no expressions
- no constants for alpha unless explicitly wanted later
- no capacity without type

Valid:

```bst
items {Int} = {}
items {Int 64} = {}
boxes {Box of String 16} = {}
```

Invalid:

```bst
items {64} = {}
items {Int String} = {}
items {Int 64 128} = {}
items {Int -1} = {}
items {Int capacity} = {}
```

## Step 2.3 — Canonical collection type representation

Replace `DataType::Collection` uses with a compiler-owned builtin generic instance:

```rust
DataType::GenericInstance {
    base: GenericBaseType::Builtin(BuiltinGenericType::Collection),
    arguments: vec![element_type],
}
```

But keep helper APIs for readability:

```rust
DataType::collection(element_type)
DataType::is_collection()
DataType::collection_element_type()
```

These are not compatibility wrappers. They are semantic constructors/helpers around the one canonical representation.

## Step 2.4 — Update all collection consumers

Replace old collection pattern matches in:

- type compatibility
- collection literal typing
- empty collection typing
- declaration type checking
- loop typing
- collection built-in method typing
- indexed write typing
- `get` result typing
- string/template boundary rejection
- HIR type lowering
- borrow checker collection paths if any
- JS backend helper expectations if they inspect AST `DataType`
- diagnostics/display

End-state:

```rust
DataType::Collection(...)
```

is deleted.

## Step 2.5 — HIR lowering remains concrete

Lower frontend collection generic instance to:

```rust
HirTypeKind::Collection { element: TypeId }
```

HIR should not store `BuiltinGenericType::Collection` as a generic instance.

## Step 2.6 — Diagnostics

Display collection types with Beanstalk syntax:

```text
{Int}
{Box of String}
```

When capacity is shown, make it clear it is an allocation hint:

```text
{Int 64}
```

But type mismatch diagnostics should compare semantic types without capacity:

```text
Expected {Int}, found {String}
```

## Phase 2 audit / style / validation commit

Audit checklist:

- `DataType::Collection` removed completely
- all collection logic goes through canonical generic instance helpers
- capacity does not affect type identity
- `{T}` syntax unchanged
- `{T capacity}` parsed and validated
- HIR still uses concrete collection type kind
- no duplicate old/new collection codepaths

Tests:

- existing collection tests still pass
- collection capacity success
- invalid capacity diagnostics
- nested allowed collection element generic: `{Box of String}`
- capacity with generic element: `{Box of String 16}`
- type identity ignores capacity
- HIR lowering still produces collection type

Validation:

```bash
just validate
```

Commit name:

```text
Migrate collections onto generic type infrastructure
```

---

# Phase 3 — Generic structs and choices

## Purpose

Phase 3 implements concrete generic nominal type instantiation for structs and choices.

Generic structs/choices remain nominal after instantiation. Instantiation is lazy and cached.

## Step 3.1 — Add generic nominal declaration storage

Store generic declarations separately from instantiated concrete declarations.

Recommended model:

```rust
pub(crate) struct GenericNominalDeclaration {
    pub path: InternedPath,
    pub parameters: GenericParameterList,
    pub kind: GenericNominalKind,
    pub location: SourceLocation,
}

pub(crate) enum GenericNominalKind {
    Struct { fields: Vec<Declaration> },
    Choice { variants: Vec<ChoiceVariant> },
}
```

This may live in AST module state or a frontend type registry.

## Step 3.2 — Add generic instantiation cache

Key:

```rust
GenericInstantiationKey {
    base_path,
    arguments,
}
```

Value:

```rust
pub(crate) enum InstantiatedNominalType {
    Struct { concrete_path_or_id, fields },
    Choice { concrete_path_or_id, variants },
}
```

Do not create fake source paths by string concatenation. Use IDs/keys. Display can use source syntax.

## Step 3.3 — Instantiate generic structs lazily

When resolving:

```bst
Box of Int
```

- resolve `Box` to canonical generic struct declaration
- validate arity
- validate type arguments are concrete/no unresolved params unless inside generic declaration context
- substitute `T -> Int` in fields
- create/cache concrete nominal instantiation

`Box of Int` and `Box of String` are distinct nominal instantiations.

## Step 3.4 — Instantiate generic choices lazily

Same as structs, but substitute through variant payload fields.

Unit variants do not carry type parameters directly, but the choice instantiation itself may still require all parameters if other variants use them.

## Step 3.5 — Generic struct constructors

Surface:

```bst
number_box Box of Int = Box(42)
text_box Box of String = Box("hello")
```

Rules:

- constructor uses base declaration name or local import alias
- infer type arguments from immediate constructor args where possible
- immediate expected type may fill missing generic args
- no inference from later uses
- if type params remain unknown, error
- constructor named/positional rules stay identical to non-generic structs

Valid:

```bst
number_box = Box(42)
explicit Box of Int = Box(42)
empty_box Box of {String} = Box({})
```

Invalid:

```bst
bad_box = Box({})
```

because `{}` does not infer `T` and there is no expected type.

## Step 3.6 — Generic choice constructors

Surface:

```bst
result ResultShape of String, Error = ResultShape::Ok("hello")
err ResultShape of String, Error = ResultShape::Err(Error(...))
```

Rules:

- infer from payload args
- immediate expected type may fill missing type params
- all type params must be known
- unit variants may require expected type if no payload carries generic params
- constructor syntax stays the same as non-generic choices

Invalid:

```bst
value = ResultShape::Ok("hello")
```

if `E` cannot be inferred.

## Step 3.7 — Pattern matching on generic choices

Pattern matching should work after scrutinee type is concrete:

```bst
result ResultShape of String, Error = parse()

if result is:
    case Ok(value) => io(value)
    case Err(error) => io(error.message)
;
```

Rules:

- match uses instantiated variant metadata
- exhaustiveness remains tag-level
- payload capture types are substituted concrete types
- no nested payload patterns
- no recursive generic choices

## Step 3.8 — Reject receiver methods on generic instances

Rejected:

```bst
describe |this Box of Int| -> String:
    return "box"
;
```

Diagnostic:

```text
Receiver methods on generic instantiated types are not supported.
Use a free function instead.
```

Generic receiver methods are also rejected:

```bst
first type T |this {T}| -> T:
    return this.get(0)!
;
```

## Phase 3 audit / style / validation commit

Audit checklist:

- generic structs/choices instantiate lazily
- instantiation cache uses structured identity
- no fake string paths
- constructor inference only uses immediate args/context
- generic choices reuse choice infrastructure after instantiation
- receiver methods on generic types rejected
- recursive generics rejected
- HIR sees concrete struct/choice types only

Tests:

- generic struct success
- generic struct constructor inference
- expected-type constructor inference
- empty collection constructor ambiguity
- wrong arity
- missing generic args
- non-generic `of` error
- generic choice success
- generic choice constructor expected context
- choice match with generic payloads
- imported/aliased generic struct/choice
- receiver method rejection
- recursive generic rejection

Validation:

```bash
just validate
```

Commit name:

```text
Implement generic structs and choices
```

---

# Phase 4 — Generic free functions

## Purpose

Phase 4 implements generic free functions with conservative pre-trait body rules.

Generic functions are relationship-preserving first. They do not become implicit trait/duck-typed functions.

Generic function templates must not be emitted as ordinary executable AST functions.
The AST stage should store the template body separately, and each call-site instantiation should
create or reuse a concrete lowering unit keyed by canonical function path plus concrete type
arguments. Executable HIR must only see concrete function IDs, concrete call targets, and concrete
`TypeId`s; unresolved `TypeParameter` or user `GenericInstance` values remain frontend-only.

## Step 4.1 — Store generic function declarations

Generic function metadata includes:

```rust
pub(crate) struct GenericFunctionDeclaration {
    pub path: InternedPath,
    pub parameters: GenericParameterList,
    pub signature: FunctionSignature,
    pub body_tokens: FileTokens,
    pub source_file: InternedPath,
    pub location: SourceLocation,
}
```

The existing function declaration path remains canonical.

## Step 4.2 — Validate generic function shape

Rules:

- all generic params used in function parameter or return types
- generic params may appear inside collections and generic instances
- generic params may not appear only in the body
- no generic receiver methods
- no trait bounds yet

## Step 4.3 — Define allowed operations on unconstrained generic parameters

Allowed:

- pass as function argument where the receiving type matches the same concrete type parameter
- return
- assign/store
- place in structs/choices/collections
- construct generic containers/nominal types
- collection-owned operations that do not require behavior from `T`

Examples allowed:

```bst
identity type T |value T| -> T:
    return value
;

wrap type T |value T| -> Box of T:
    return Box(value)
;

first type T |items {T}| -> T:
    return items.get(0)!
;

append type T |items ~{T}, value T|:
    ~items.push(value)
;
```

Rejected until traits:

```bst
add_one type T |value T| -> T:
    return value + 1
;

equals type T |left T, right T| -> Bool:
    return left is right
;

stringify type T |value T| -> String:
    return value.to_string()
;
```

Reason: operators, equality, methods, casts, ordering, string conversion, sorting, and behavior-dependent calls require trait constraints.

## Step 4.4 — Add generic body validation mode

AST body parsing needs a mode where generic parameters are known but unconstrained.

Add a rule layer to expression/type checking:

```rust
GenericBodyValidationMode::Unconstrained
```

or equivalent context flag.

This mode rejects behavior-dependent operations involving `DataType::TypeParameter` unless the operation is explicitly allowed as structural movement.

Do not implement a temporary per-concrete “try it and see” body checker. Traits are coming soon, so avoid scaffolding that will be removed.

## Step 4.5 — Generic function call inference

Inference sources:

1. immediate call arguments
2. immediate expected type context from declaration target
3. immediate expected type context from return slot

Never infer from later use.

Valid:

```bst
x = identity(42)
name String = identity("Nye")
items {String} = make_empty()
```

Invalid:

```bst
items = make_empty()
items.push("name")
```

`items.push` must not help infer `make_empty`.

## Step 4.6 — Generic calls inside generic functions

Allowed when fully inferable from local immediate context.

```bst
identity type T |value T| -> T:
    return value
;

wrap_identity type T |value T| -> Box of T:
    inner = identity(value)
    return Box(inner)
;
```

Reject if any type argument remains unknown.

## Step 4.7 — Instantiate generic functions before HIR

Maintain cache:

```rust
GenericFunctionInstantiationKey {
    function_path: InternedPath,
    arguments: Vec<TypeIdentityKey>,
}
```

The cache stores the concrete typed AST/lowered function representation needed by HIR.

Do not make this a backend monomorphization policy. It is frontend concrete typing so HIR remains concrete.

## Step 4.8 — HIR lowering

HIR lowering sees concrete functions/calls only.

Options:

1. emit concrete synthetic function entries for each instantiation before HIR module finalization
2. lower instantiated call bodies inline into normal function table under concrete IDs

Preferred: concrete synthetic function entries, because borrow validation and backend codegen already expect function bodies and calls.

Use stable internal IDs, not stringified names.

Display/debug can render:

```text
identity of Int
wrap_identity of String
```

but backend identity should be structured.

## Step 4.9 — Diagnostics

Generic inference failures should name the unknown parameter:

```text
Cannot infer generic parameter T for call to make_empty.
Add an explicit declaration type, such as `items {String} = make_empty()`.
```

Generic body rule failures should point at the behavior-dependent operation:

```text
Generic parameter T has no trait bounds, so `+` cannot be used here.
Add a trait bound when traits are implemented, or restrict this function to a concrete type.
```

## Phase 4 audit / style / validation commit

Audit checklist:

- generic functions are free functions only
- no generic receiver methods
- no behavior-dependent operations on unconstrained `T`
- inference limited to immediate args/context
- no inference from later uses
- instantiation cache is structured
- HIR remains concrete
- diagnostics explain trait-deferred gaps

Tests:

- identity generic function
- generic return preservation
- generic collection function `first`
- generic function expected return context
- generic call inside generic function
- inference failure
- later-use inference rejection
- operator/equality/method/cast rejection on `T`
- imported generic function
- aliased generic function import
- exported generic function through `#mod.bst`

Validation:

```bash
just validate
```

Commit name:

```text
Implement conservative generic free functions
```

---

# Phase 5 — Type aliases to concrete generic instances

## Purpose

Phase 5 completes concrete generic aliases and ensures aliases remain transparent.

This can be merged into Phase 1 if small, but keeping it separate makes review cleaner.

## Supported

```bst
StringBox as Box of String
StringIntMap as Map of String, Int
BoxedStringIntMap as Box of StringIntMap
Names as {String}
StringBoxes as {Box of String}
```

## Rejected

```bst
BoxedMap as Box of Map
Response type T as ResultShape of T, Error
MaybePair type A, B as Pair of A, B
```

## Step 5.1 — Alias target resolution

Alias targets resolve through the same `TypeResolutionContext` as declarations.

Concrete generic alias target requirements:

- no unresolved generic parameters
- no partially applied generic declarations
- no generic alias parameters
- no recursive alias cycles
- nested `of` rule still applies

## Step 5.2 — Alias import/export behavior

Existing alias import/export rules remain:

- imported aliases are file-local
- exporting an imported alias under a public name requires declaring a real exported type alias
- aliases are transparent
- alias name collisions are hard errors

## Step 5.3 — Diagnostics

Wrong:

```bst
BoxedMap as Box of Map
```

Diagnostic:

```text
Generic aliases and partial generic application are not supported.
Create an alias to a fully concrete type, such as `StringIntMap as Map of String, Int`.
```

## Phase 5 audit / style / validation commit

Audit checklist:

- no generic alias support slipped in
- concrete generic aliases use canonical type resolver
- aliases remain transparent
- alias cycles still caught

Tests:

- alias to `Box of String`
- alias to `{Box of String}`
- alias to imported generic instantiation
- exported alias in `#mod.bst`
- partial alias rejection
- generic alias rejection
- alias cycle through generic instance rejection

Validation:

```bash
just validate
```

Commit name:

```text
Support aliases to concrete generic instances
```

---

# Phase 6 — Documentation and progress matrix updates

## Purpose

Update user-facing and roadmap documentation to match implemented and deliberately deferred behavior.

## Step 6.1 — Update `docs/src/docs/generics/#page.bst`

Keep design draft disclaimer if the feature is still incomplete. Update examples/rules:

- `type` introduces generic parameters.
- `of` applies generic type arguments in type positions only.
- Collections keep `{T}` syntax and support `{T capacity}`.
- Collection capacity is an allocation hint, not part of type identity.
- Inline nested `of` applications are rejected.
- Compose complex generic types through concrete type aliases.
- Generic aliases / partial type application are deferred.
- Generic receiver methods are deferred.
- Receiver methods on generic instances are deferred.
- Recursive generic nominal types are deferred.
- Behavior-dependent operations on unconstrained generic params wait for traits.
- Generic functions support type-preserving movement only until traits exist.

Correct docs examples:

```bst
Box type T = |
    value T,
|

Pair type Left, Right = |
    left Left,
    right Right,
|

StringBox as Box of String
StringIntPair as Pair of String, Int
boxes {StringBox 32} = {}
```

Avoid examples like:

```bst
Box of Map of String, Int
Map of String, Box of Int
BoxedMap as Box of Map
identity of Int(42)
```

unless they are explicitly in rejected/deferred sections.

## Step 6.2 — Update language overview

Update `docs/language-overview.md` and/or relevant docs pages with concise syntax notes:

- generic declarations
- generic type application
- collection syntax exception
- capacity syntax
- alias examples
- deferred trait-bound behavior

## Step 6.3 — Update compiler design docs

Update `docs/compiler-design-overview.md` with:

- generics are solved before HIR
- HIR executable bodies remain concrete
- collection frontend generic machinery lowers to concrete HIR collection kind
- generic declarations live in header/module symbol metadata
- type resolution goes through `TypeResolutionContext`

## Step 6.4 — Update progress matrix

In `docs/src/docs/progress/#page.bst`, split generics into explicit rows.

Suggested rows:

| Surface | Status | Coverage | Runtime target | Watch points |
|---|---|---|---|---|
| Generic type infrastructure | Supported once Phase 0 lands | Targeted internal tests | Frontend-owned | Internal substrate only; no user syntax in Phase 0 except reserved keywords. |
| Generic declarations | Partial/Supported depending on phase | Targeted parser + integration | Frontend-owned | `type` params for functions/structs/choices. No bounds. |
| Generic structs | Supported after Phase 3 | Broad integration | JS / HTML through concrete HIR | Lazy instantiated, nominal, no recursive generics. |
| Generic choices | Supported after Phase 3 | Broad integration | JS / HTML through concrete HIR | Reuses choice machinery after instantiation. No recursive generics. |
| Generic functions | Partial after Phase 4 | Targeted/Broad | JS / HTML through concrete HIR | Type-preserving structural movement only until traits. |
| Generic type aliases | Partial | Targeted | Frontend-owned | Only aliases to fully concrete generic instances. Generic aliases deferred. |
| Collection generic substrate | Supported after Phase 2 | Broad existing + new capacity tests | JS / HTML | `{T}` remains syntax. Capacity is allocation metadata. |
| Trait bounds on generics | Deferred | None | Frontend-owned future | Required for operators/equality/methods on `T`. |
| Explicit generic call-site application | Deferred | Diagnostics only | Frontend-owned future | `identity of Int(42)` rejected. |
| Generic receiver methods | Deferred | Diagnostics only | Frontend-owned future | Use free functions. |
| Recursive generic types | Deferred | Diagnostics only | Frontend-owned future | Requires layout/indirection design. |
| Nested generic type application | Rejected / Reserved | Diagnostics | Frontend-owned | Name intermediate concrete aliases. |
| Generic aliases / partial type application | Deferred | Diagnostics | Frontend-owned | Avoid type-constructor composition for now. |

Remove or update older “Choice generic declarations: Deferred” row once generic choices land.

## Step 6.5 — Add roadmap links

If the docs site has a roadmap/plans directory, add this plan or a Beanstalk page linking to it:

```text
docs/roadmap/plans/generics-implementation-plan.md
```

or docs page equivalent.

## Phase 6 audit / style / validation commit

Audit checklist:

- docs match actual syntax
- deferred features are explicit
- examples avoid unsupported nested `of`
- progress matrix rows reflect real implementation state
- plan links added where appropriate
- docs build passes

Validation:

```bash
just validate
```

Commit name:

```text
Document generic implementation scope
```

---

# Phase 7 — End-to-end hardening pass

## Purpose

Once the feature works, do not rush onward to traits. First harden generics as a compiler feature.

## Step 7.1 — Cross-feature integration tests

Add integration tests for:

- generic structs across files
- generic choices across files
- generic functions across files
- `#mod.bst` re-export of generic declarations
- import aliases on generic declarations
- type aliases to generic instances
- collections of generic instances
- generic instances containing collections
- generic functions returning generic choices
- generic choices carrying generic structs
- templates rejecting unsupported generic complex values at string boundaries
- borrow validation with generic-instantiated concrete types

## Step 7.2 — Negative diagnostics

Add diagnostics for:

- `of` in expressions
- missing generic args
- extra generic args
- wrong arity
- non-generic `of`
- nested `of`
- generic alias syntax
- partial generic alias target
- generic receiver methods
- receiver methods on generic instances
- recursive generics
- unused generic parameters
- generic parameter collision
- non-PascalCase generic parameter
- behavior-dependent operation on unconstrained `T`
- inference failure from ambiguous empty collection
- inference failure where only later use would help

## Step 7.3 — Code quality audit

Check for:

- large functions over ~200 lines that should split
- duplicated resolution logic
- duplicated arity checks
- duplicated generic inference code
- old collection codepaths after Phase 2
- generic-specific hacks in `call_validation.rs`, `type_coercion`, or HIR lowering
- user-data-driven panics/unwraps
- missing comments around instantiation cache and type identity

## Step 7.4 — Performance smoke

Generic instantiation can easily regress compile speed.

Add or extend smoke cases:

- many repeated `Box of Int` uses should hit cache
- many distinct instantiations should remain linear-ish
- docs build should not slow materially
- `benchmarks/speed-test.bst` should not degrade heavily

Use existing validation commands with detailed timers:

```bash
cargo run --features "detailed_timers" docs
cargo run --release --features "detailed_timers" check benchmarks/speed-test.bst
```

## Phase 7 audit / style / validation commit

Audit checklist:

- broad positive and negative integration coverage
- no duplicate generic inference/resolution paths
- collection old representation gone
- HIR concrete-type contract maintained
- compile-time impact measured
- diagnostics are clear and specific

Validation:

```bash
just validate
```

Commit name:

```text
Harden generic implementation
```

---

# Phase 8 — Trait-bound integration planning hook

## Purpose

Traits are intentionally close behind generics. This phase should not implement traits unless that work is already underway. It should prepare the explicit handoff.

## Step 8.1 — Add deferred trait-bound tasks

In roadmap/progress docs, list:

- trait declarations
- trait-bound syntax
- trait-bound type resolution
- behavior-dependent generic operations
- equality/ordering/string conversion traits
- generic methods only if still desired
- external trait implementation model, if any

## Step 8.2 — Mark generic gaps as trait-owned

Document that these remain rejected until traits:

```bst
add type T |a T, b T| -> T:
    return a + b
;

equals type T |a T, b T| -> Bool:
    return a is b
;

stringify type T |value T| -> String:
    return value.to_string()
;
```

## Step 8.3 — Avoid temporary scaffolding

Do not add fake implicit traits or per-instantiation duck typing just to make these examples work. That would need removal later and would blur Beanstalk semantics.

## Phase 8 audit / style / validation commit

Audit checklist:

- generics docs point to trait-bound future work
- diagnostics explain “requires future trait bounds”
- no implicit behavior constraints were added

Validation:

```bash
just validate
```

Commit name:

```text
Mark generic trait-bound followups
```

---

# Implementation details by compiler area

## Tokenizer

Files:

```text
src/compiler_frontend/tokenizer/tokens.rs
src/compiler_frontend/tokenizer/lexer.rs
```

Changes:

- add `TokenKind::Type`
- add `TokenKind::Of`
- reserve `type` and `of`
- update keyword helper tests
- update unsupported keyword diagnostics where relevant

## Type model

Files:

```text
src/compiler_frontend/datatypes.rs
src/compiler_frontend/types/generics.rs
```

Changes:

- add `TypeParameter`
- add `GenericInstance`
- add `GenericBaseType`
- add `BuiltinGenericType::Collection`
- add `GenericParameter*` types
- add `TypeSubstitution`
- add `TypeIdentityKey`
- update display/equality/traversal
- remove `DataType::Collection` in Phase 2

## Type syntax

File:

```text
src/compiler_frontend/declaration_syntax/type_syntax.rs
```

Changes:

- refactor to atom/postfix layers
- parse `of`
- enforce one-level generic application rule
- parse collection capacity
- keep `?` suffix after type application
- reject `of` outside type positions
- reject nested `of` with targeted diagnostic
- resolve through `TypeResolutionContext`

## Header parsing

Files:

```text
src/compiler_frontend/headers/types.rs
src/compiler_frontend/headers/header_dispatch.rs
src/compiler_frontend/declaration_syntax/struct.rs
src/compiler_frontend/declaration_syntax/choice.rs
src/compiler_frontend/ast/statements/functions.rs
```

Changes:

- add `generic_parameters` to header kinds
- parse `type T, U`
- register metadata
- reject generic aliases
- preserve dependency edges from generic parameterized type shapes

## Module symbols

File:

```text
src/compiler_frontend/headers/module_symbols.rs
```

Changes:

- add generic declaration metadata registry
- update declaration placeholders for generic declarations
- ensure import/facade maps keep canonical generic declaration paths

## AST type resolution

Files likely involved:

```text
src/compiler_frontend/ast/type_resolution.rs
src/compiler_frontend/ast/module_ast/pass_type_alias_resolution.rs
src/compiler_frontend/ast/module_ast/pass_function_signatures.rs
src/compiler_frontend/ast/import_bindings.rs
```

Changes:

- introduce and use `TypeResolutionContext`
- resolve generic parameters before normal visible types
- instantiate generic nominal types lazily
- cache instantiated structs/choices
- resolve concrete generic aliases
- reject collisions, unused params, recursive generics

## AST expressions/calls

Files likely involved:

```text
src/compiler_frontend/ast/expressions/call_validation.rs
src/compiler_frontend/ast/expressions/choice_constructor.rs
src/compiler_frontend/ast/statements/declarations.rs
src/compiler_frontend/ast/statements/functions.rs
```

Changes:

- infer generic struct constructor args
- infer generic choice constructor args
- infer generic function calls
- support expected type context at declarations/returns
- reject later-use inference
- reject `of` expression syntax
- reject behavior-dependent operations on `T`

## Type coercion

Files:

```text
src/compiler_frontend/type_coercion/compatibility.rs
src/compiler_frontend/type_coercion/numeric.rs
src/compiler_frontend/type_coercion/string.rs
```

Changes:

- compare concrete generic instantiations by identity
- preserve alias transparency
- collection compatibility goes through collection helper APIs after Phase 2
- no implicit coercion between `Box of Int` and `Box of Float`
- no trait-like behavior coercions

## HIR

Files:

```text
src/compiler_frontend/hir/hir_datatypes.rs
src/compiler_frontend/hir/hir_builder.rs
src/compiler_frontend/hir/*
```

Changes:

- keep executable HIR concrete
- lower instantiated generic structs/choices as concrete nominal HIR types
- lower collection generic substrate to `HirTypeKind::Collection`
- do not add unresolved type parameters to executable HIR
- ensure display/debug can show concrete instantiated origins if useful

## Backends

JS/HTML should receive concrete HIR and need minimal generic-specific logic.

If backend changes are needed, they should be for:

- concrete synthetic function names/IDs
- concrete instantiated struct/choice metadata
- collection capacity allocation hints, if JS helper supports it

Do not put generic solving in JS/Wasm backends.

---

# Deferred feature diagnostics to add

Use structured diagnostics for each intentionally unsupported surface.

| Surface | Diagnostic direction |
|---|---|
| `identity of Int(42)` | Explicit generic function application is not supported. Generic function calls infer type arguments from immediate arguments/context. |
| `Box of Int(42)` in expression | `of` is only valid in type positions. Use `value Box of Int = Box(42)`. |
| nested `of` | Nested generic applications are not supported in one annotation. Name the inner type with a concrete alias. |
| partial generic alias | Generic aliases / partial generic application are not supported. Alias a fully concrete type. |
| generic alias params | Generic type aliases are not supported. |
| generic receiver method | Generic receiver methods are not supported. Use a free function. |
| receiver on generic instance | Receiver methods on generic instantiated types are not supported. Use a free function. |
| recursive generic type | Recursive generic types are deferred. |
| behavior operation on `T` | This operation requires future trait bounds. Unconstrained generic parameters only support type-preserving structural movement. |
| inference from later use | Generic inference only uses immediate call arguments and immediate declaration/return context. Add an explicit type annotation. |
| collection capacity malformed | Collection capacity annotations require exactly one integer literal after the element type. |

---

# Test plan summary

## Unit tests

- generic metadata construction
- generic scope collision/name validation
- type substitution
- type identity keys
- `DataType` display/equality
- type syntax parser
- collection capacity parser
- type resolver context

## Integration tests

- generic structs
- generic choices
- generic functions
- imports/re-exports
- aliases to generic instances
- collection migration/capacity
- generic values through templates/string rejection
- borrow validation over concrete instantiations

## Negative tests

- wrong arity
- missing args
- extra args
- non-generic `of`
- nested `of`
- `of` in expressions
- generic alias params
- partial generic aliases
- recursive generic types
- generic receiver methods
- concrete generic receiver methods
- behavior-dependent `T` operations
- inference failure
- later-use inference rejection
- generic param naming/collision/unused errors

---

# Final success condition

The feature is in good shape when all of these are true:

```text
The compiler has one generic-ready type model.
Collections use the generic substrate internally and no old DataType::Collection path remains.
Generic structs and choices instantiate lazily and nominally.
Generic free functions preserve type relationships without pretending traits exist.
Generic solving is complete before HIR executable lowering.
HIR and backends receive concrete types/functions only.
Docs and progress matrix explicitly mark every deferred generic feature.
Diagnostics are direct, structured, and do not collapse into vague type mismatch cascades.
```

This keeps generics useful without letting them sprawl into a Rust/C++/TypeScript-style complexity trap.
