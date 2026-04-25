# Beanstalk Phase 3 Implementation Plan

## Title

**Phase 3: Separate semantic type identity from frontend value/access/ownership state**

## Purpose

Phase 3 removes access/ownership state from `DataType` so the frontend type model matches the memory-management design more closely.

The target invariant is:

> `DataType` describes semantic type identity only. Mutability, shared/reference status, ownership eligibility, and call-site access intent live outside the type.

This is grounded in the current design docs:

- GC is the semantic baseline.
- Ownership is an optimization target, not a static type distinction.
- Runtime ownership metadata is optional backend state.
- Borrow validation and later lowering decide ownership/drop behavior from HIR facts and analysis, not from static frontend type identity.

The HIR layer already mostly follows this rule: `HirTypeKind::Collection { element }` and `HirTypeKind::Struct { struct_id }` do not carry ownership, while `HirLocal` separately carries `mutable: bool`.

Phase 3 aligns AST/frontend `DataType` with that cleaner HIR boundary.

---

## Current repo-grounded state

### Relevant files

| File | Current role |
|---|---|
| `src/compiler_frontend/datatypes.rs` | Defines `DataType`, `Ownership`, receiver keys, display, equality |
| `src/compiler_frontend/type_coercion/compatibility.rs` | Central type compatibility predicate |
| `src/compiler_frontend/declaration_syntax/type_syntax.rs` | Parses type annotations and currently constructs `DataType::Collection(_, Ownership)` |
| `src/compiler_frontend/declaration_syntax/declaration_shell.rs` | Stores declaration mutability and currently adjusts collection type ownership |
| `src/compiler_frontend/ast/expressions/expression.rs` | Stores `Expression { data_type, ownership, ... }` and constructs collection/struct types |
| `src/compiler_frontend/ast/ast_nodes.rs` | Stores `MultiBindTarget { data_type, ownership, ... }` and field-access metadata |
| `src/compiler_frontend/headers/module_symbols.rs` | Constructs declaration placeholders using `DataType::runtime_struct` and collection return types |
| `src/compiler_frontend/builtins/error_type.rs` | Constructs builtin struct/error declarations |
| `src/compiler_frontend/ast/type_resolution.rs` | Resolves named types and struct fields |
| `src/compiler_frontend/ast/statements/loops.rs` | Creates loop binding declarations and collection/range-related types |
| `src/compiler_frontend/ast/field_access/collection_builtin.rs` | Reads collection element types and validates collection methods |
| `src/compiler_frontend/ast/expressions/call_validation.rs` | Validates explicit mutable argument behavior |
| `src/compiler_frontend/hir/hir_expression/types.rs` | Lowers frontend `DataType` to canonical `HirTypeKind` |
| `src/compiler_frontend/hir/hir_datatypes.rs` | Canonical HIR type model; already no ownership in type identity |

If Phase 1 split `hir_nodes.rs` or `datatypes.rs`, apply the same symbol changes in the new file locations. The symbols and responsibilities are the important part.

---

## Problem statement

`DataType` currently carries value/access/ownership state in at least these variants:

```rust
pub enum DataType {
    Collection(Box<DataType>, Ownership),
    Struct {
        nominal_path: InternedPath,
        fields: Vec<Declaration>,
        ownership: Ownership,
        const_record: bool,
    },
    ...
}
```

This causes several problems:

1. **Type identity and access state are mixed.**
   A mutable collection and immutable collection are currently different `DataType` values by structural equality.

2. **`PartialEq` bakes access into type equality.**
   `DataType::Collection(a, oa) == DataType::Collection(b, ob)` requires `oa == ob`.

3. **Struct compatibility has a workaround.**
   `type_coercion::compatibility::is_type_compatible` already special-cases structs to ignore ownership. That is a symptom that ownership should not be in `DataType`.

4. **Declaration syntax mutates type shape based on binding mutability.**
   `DeclarationSyntax::to_data_type(&Ownership)` adjusts collection ownership inside the type annotation.

5. **HIR already ignores this ownership data.**
   `HirBuilder::lower_data_type` lowers `DataType::Collection(inner, _)` to `HirTypeKind::Collection { element }`, discarding the ownership payload.

The cleanup is therefore not speculative. It removes frontend state that later stages already treat as non-type metadata.

---

## Desired final contract

### `DataType`

`DataType` should be pure semantic type identity:

```rust
pub enum DataType {
    Inferred,
    NamedType(StringId),

    Collection(Box<DataType>),
    Struct {
        nominal_path: InternedPath,
        fields: Vec<Declaration>,
        const_record: bool,
    },
    Reference(Box<DataType>),
    Range,
    Returns(Vec<DataType>),
    Function(Box<Option<ReceiverKey>>, FunctionSignature),

    Path(PathTypeKind),
    Template,

    Bool,
    Int,
    Float,
    Decimal,
    StringSlice,
    Char,
    BuiltinErrorKind,

    Parameters(Vec<Declaration>),
    Choices {
        nominal_path: InternedPath,
        variants: Vec<ChoiceVariant>,
    },
    Option(Box<DataType>),
    Result {
        ok: Box<DataType>,
        err: Box<DataType>,
    },
    TemplateWrapper,
    None,
    True,
    False,
}
```

### Frontend value/access mode

Replace the misleading `Ownership` name with a frontend-local value/access classification.

Recommended name:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ValueMode {
    MutableOwned,
    MutableReference,
    #[default]
    ImmutableOwned,
    ImmutableReference,
}
```

Recommended module:

```text
src/compiler_frontend/value_mode.rs
```

or, if Phase 1 created a `types/` directory:

```text
src/compiler_frontend/types/value_mode.rs
```

`ValueMode` carries current frontend expression/binding classification only. It is not type identity.

Recommended methods:

```rust
impl ValueMode {
    pub fn is_mutable(&self) -> bool {
        matches!(self, Self::MutableOwned | Self::MutableReference)
    }

    pub fn as_owned(&self) -> Self {
        match self {
            Self::MutableReference => Self::MutableOwned,
            Self::ImmutableReference => Self::ImmutableOwned,
            _ => self.clone(),
        }
    }

    pub fn as_reference(&self) -> Self {
        match self {
            Self::MutableOwned => Self::MutableReference,
            Self::ImmutableOwned => Self::ImmutableReference,
            _ => self.clone(),
        }
    }
}
```

This preserves the current operational meaning while removing it from the semantic type model.

### AST expression/binding state

These still carry value/access state:

```rust
pub struct Expression {
    pub data_type: DataType,
    pub value_mode: ValueMode,
    ...
}
```

```rust
pub struct MultiBindTarget {
    pub data_type: DataType,
    pub value_mode: ValueMode,
    ...
}
```

If renaming `ownership` to `value_mode` is too large for one edit, keep the field name temporarily but change its type to `ValueMode`. However, the final Phase 3 check should require that `Ownership` no longer exists as a frontend type name. Otherwise the code will keep implying that runtime ownership is a static language type concept.

### HIR

HIR remains mostly unchanged:

- `HirTypeKind::Collection { element }` remains pure.
- `HirTypeKind::Struct { struct_id }` remains pure.
- `HirLocal { mutable: bool }` continues to represent local mutability.
- borrow checker facts continue to carry access/borrow state.

---

## Non-goals

Phase 3 must not include:

- Changing runtime memory behavior
- Implementing last-use analysis
- Implementing runtime ownership flags
- Adding deterministic drops
- Redesigning borrow checker facts
- Changing syntax
- Changing mutable call-site rules
- Removing explicit `~`
- Changing HIR `TypeId` representation
- Replacing AST `DataType` with HIR `TypeId`
- Reworking Phase 2 declaration ordering

---

# Mandatory preconditions

Before starting Phase 3, Phase 2 must be complete.

Run these required searches:

```bash
rg "declaration_stubs_by_path"
rg "DeclarationStub"
rg "DeclarationStubKind"
rg "seed_declaration_stubs"
```

Expected result: no matches.

Run these required checks:

```bash
cargo check
cargo test
cargo run tests
```

Do not start Phase 3 if Phase 2 cleanup is only partially complete. Phase 3 will touch the same declaration/type paths, so stacking it on unfinished Phase 2 work will make breakages hard to isolate.

---

# Implementation plan

## Step 1 — Add type/access separation guard tests

Add tests before the main rewrite.

### 1.1 Type compatibility tests

Edit:

```text
src/compiler_frontend/type_coercion/tests/compatibility_tests.rs
```

Add tests for the intended pure type behavior.

After the rewrite, examples should look like:

```rust
#[test]
fn collection_type_identity_ignores_value_mode() {
    let left = DataType::Collection(Box::new(DataType::Int));
    let right = DataType::Collection(Box::new(DataType::Int));

    assert_eq!(left, right);
    assert!(is_type_compatible(&left, &right));
}
```

For structs:

```rust
#[test]
fn struct_type_identity_is_nominal_and_const_record_sensitive_only() {
    let path = test_path("User");

    let runtime = DataType::runtime_struct(path.clone(), vec![]);
    let same_runtime = DataType::runtime_struct(path.clone(), vec![]);
    let const_record = DataType::const_struct_record(path, vec![]);

    assert_eq!(runtime, same_runtime);
    assert_ne!(runtime, const_record);
}
```

Use existing test helpers if available. If path helpers do not exist in this module, add a tiny local `StringTable` + `InternedPath` helper.

### 1.2 HIR lowering tests

Edit or add tests under:

```text
src/compiler_frontend/hir/tests/
```

Add a test that lowering a collection/struct type does not require value/access state.

The key assertion is not about exact `TypeId` numbers. It should assert the lowered `HirTypeKind`:

```rust
DataType::Collection(Box::new(DataType::Int))
```

lowers to:

```rust
HirTypeKind::Collection { element: int_type_id }
```

For structs, assert only nominal struct identity is used.

### 1.3 Integration tests for behavior preservation

Add integration fixtures if equivalent coverage is not already strong enough.

Required behavior surfaces:

1. Immutable collection can be passed to a shared collection parameter.
2. Mutable collection can be passed to the same shared collection parameter.
3. Mutable collection method still requires explicit `~`.
4. Mutable struct field assignment still works.
5. Function expecting a struct accepts both immutable and mutable bindings of that struct type.
6. Borrow checker still rejects overlapping mutable/shared access.

Candidate fixture names:

```text
tests/cases/type_identity_mutable_collection_shared_param
tests/cases/type_identity_mutable_struct_shared_param
tests/cases/type_identity_collection_mutation_rules_preserved
tests/cases/type_identity_struct_field_mutation_rules_preserved
```

Do not over-test if equivalent cases already exist. The mandatory check is that the behavior is covered somewhere in integration tests.

---

## Step 2 — Introduce `ValueMode`

Create:

```text
src/compiler_frontend/value_mode.rs
```

or, if Phase 1 moved type-related files:

```text
src/compiler_frontend/types/value_mode.rs
```

Recommended contents:

```rust
//! Frontend value/access classification.
//!
//! WHAT: carries AST-level mutability/reference/owned-vs-borrowed classification for expressions,
//! declarations, and binding targets.
//!
//! WHY: semantic type identity must not carry access or ownership state. `ValueMode` keeps this
//! data attached to values/bindings while `DataType` remains a pure type description.
//!
//! This is not the final runtime ownership flag model. Runtime ownership remains a later lowering
//! concern driven by borrow/last-use analysis.

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ValueMode {
    MutableOwned,
    MutableReference,
    #[default]
    ImmutableOwned,
    ImmutableReference,
}

impl ValueMode {
    pub fn is_mutable(&self) -> bool {
        matches!(self, Self::MutableOwned | Self::MutableReference)
    }

    pub fn as_owned(&self) -> Self {
        match self {
            Self::MutableReference => Self::MutableOwned,
            Self::ImmutableReference => Self::ImmutableOwned,
            _ => self.clone(),
        }
    }

    pub fn as_reference(&self) -> Self {
        match self {
            Self::MutableOwned => Self::MutableReference,
            Self::ImmutableOwned => Self::ImmutableReference,
            _ => self.clone(),
        }
    }
}
```

Register it in:

```text
src/compiler_frontend/mod.rs
```

```rust
pub(crate) mod value_mode;
```

If Phase 1 introduced a `types` module, expose it through that module instead.

---

## Step 3 — Move `Ownership` users to `ValueMode`

This is a mechanical rename of frontend value metadata.

### Required global replacement

Replace imports like:

```rust
use crate::compiler_frontend::datatypes::{DataType, Ownership};
```

with:

```rust
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::value_mode::ValueMode;
```

Then replace:

```rust
Ownership::MutableOwned
Ownership::MutableReference
Ownership::ImmutableOwned
Ownership::ImmutableReference
```

with:

```rust
ValueMode::MutableOwned
ValueMode::MutableReference
ValueMode::ImmutableOwned
ValueMode::ImmutableReference
```

Replace method calls:

```rust
ownership.get_owned()
ownership.get_reference()
```

with:

```rust
value_mode.as_owned()
value_mode.as_reference()
```

### Rename fields where practical

Recommended field renames:

| Current | New |
|---|---|
| `Expression.ownership` | `Expression.value_mode` |
| `MultiBindTarget.ownership` | `MultiBindTarget.value_mode` |
| `NodeKind::FieldAccess { ownership }` | `NodeKind::FieldAccess { value_mode }` |
| local variable `ownership` used for expression/binding state | `value_mode` |

This is broad but mechanical. It is worth doing because leaving a field called `ownership` will keep the old conceptual confusion alive.

### Required search after rename

```bash
rg "Ownership"
```

Expected result after this step: no matches, except possibly in docs explaining old terminology or migration notes. Code should have no `Ownership` symbol.

If code still needs a runtime ownership concept later, it should be introduced separately in the relevant lowering/backend phase, not reused here.

---

## Step 4 — Remove value mode from `DataType`

Edit:

```text
src/compiler_frontend/datatypes.rs
```

### Change `Collection`

Before:

```rust
Collection(Box<DataType>, Ownership),
```

After:

```rust
Collection(Box<DataType>),
```

### Change `Struct`

Before:

```rust
Struct {
    nominal_path: InternedPath,
    fields: Vec<Declaration>,
    ownership: Ownership,
    const_record: bool,
},
```

After:

```rust
Struct {
    nominal_path: InternedPath,
    fields: Vec<Declaration>,
    const_record: bool,
},
```

### Update constructors

Before:

```rust
pub fn runtime_struct(
    nominal_path: InternedPath,
    fields: Vec<Declaration>,
    ownership: Ownership,
) -> Self
```

After:

```rust
pub fn runtime_struct(
    nominal_path: InternedPath,
    fields: Vec<Declaration>,
) -> Self
```

Before:

```rust
pub fn const_struct_record(nominal_path: InternedPath, fields: Vec<Declaration>) -> Self {
    Self::Struct {
        nominal_path,
        fields,
        ownership: Ownership::ImmutableOwned,
        const_record: true,
    }
}
```

After:

```rust
pub fn const_struct_record(nominal_path: InternedPath, fields: Vec<Declaration>) -> Self {
    Self::Struct {
        nominal_path,
        fields,
        const_record: true,
    }
}
```

### Remove `struct_ownership`

Delete:

```rust
pub fn struct_ownership(&self) -> Option<&Ownership>
```

Any call sites must be changed to read value mode from the expression/declaration context instead.

### Update `display_with_table`

Before:

```rust
DataType::Collection(inner_type, _mutable) => { ... }
```

After:

```rust
DataType::Collection(inner_type) => { ... }
```

Struct display remains nominal.

### Update `PartialEq`

Before:

```rust
(DataType::Collection(a, oa), DataType::Collection(b, ob)) => a == b && oa == ob,
```

After:

```rust
(DataType::Collection(a), DataType::Collection(b)) => a == b,
```

Before:

```rust
DataType::Struct {
    nominal_path: path_a,
    ownership: ownership_a,
    const_record: const_a,
    ..
},
DataType::Struct {
    nominal_path: path_b,
    ownership: ownership_b,
    const_record: const_b,
    ..
},
) => path_a == path_b && ownership_a == ownership_b && const_a == const_b,
```

After:

```rust
DataType::Struct {
    nominal_path: path_a,
    const_record: const_a,
    ..
},
DataType::Struct {
    nominal_path: path_b,
    const_record: const_b,
    ..
},
) => path_a == path_b && const_a == const_b,
```

### Update module docs

Add a top-level module doc to `datatypes.rs`:

```rust
//! Frontend semantic type model.
//!
//! WHAT: defines AST/frontend type identity before HIR type interning.
//! WHY: AST needs a rich type surface for named types, unresolved placeholders,
//! templates, choices, constants, and frontend-only wrappers.
//!
//! Access/mutability/owned-vs-reference state does not live in `DataType`.
//! That state belongs to expressions, declarations, call arguments, HIR locals,
//! and borrow-analysis facts.
```

---

## Step 5 — Update declaration syntax

Edit:

```text
src/compiler_frontend/declaration_syntax/declaration_shell.rs
```

### Remove `Ownership` import

Before:

```rust
use crate::compiler_frontend::datatypes::{DataType, Ownership};
```

After:

```rust
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::value_mode::ValueMode;
```

### Replace `DeclarationSyntax::to_data_type`

Current:

```rust
pub fn to_data_type(&self, declaration_ownership: &Ownership) -> DataType {
    match &self.type_annotation {
        DataType::Collection(inner, _) => {
            if matches!(inner.as_ref(), DataType::Inferred) {
                DataType::Collection(Box::new(*inner.clone()), Ownership::MutableOwned)
            } else {
                DataType::Collection(Box::new(*inner.clone()), declaration_ownership.clone())
            }
        }
        other => other.clone(),
    }
}
```

Replace with:

```rust
pub fn value_mode(&self) -> ValueMode {
    if self.mutable_marker {
        ValueMode::MutableOwned
    } else {
        ValueMode::ImmutableOwned
    }
}

pub fn semantic_type(&self) -> DataType {
    self.type_annotation.clone()
}
```

### Why

Declaration mutability belongs to `ValueMode`. It should not mutate the type annotation.

---

## Step 6 — Update type syntax parsing and named-type traversal

Edit:

```text
src/compiler_frontend/declaration_syntax/type_syntax.rs
```

### Collection parse

Before:

```rust
Ok(DataType::Collection(
    Box::new(inner_type),
    crate::compiler_frontend::datatypes::Ownership::ImmutableOwned,
))
```

After:

```rust
Ok(DataType::Collection(Box::new(inner_type)))
```

### Named type traversal

Before:

```rust
DataType::Collection(inner, _) | DataType::Option(inner) | DataType::Reference(inner) => {
    ...
}
```

After:

```rust
DataType::Collection(inner) | DataType::Option(inner) | DataType::Reference(inner) => {
    ...
}
```

### Named type resolution

Before:

```rust
DataType::Collection(inner, ownership) => Ok(DataType::Collection(
    Box::new(resolve_named_types_in_data_type(...)?),
    ownership.to_owned(),
)),
```

After:

```rust
DataType::Collection(inner) => Ok(DataType::Collection(Box::new(
    resolve_named_types_in_data_type(inner, location, resolve_by_name, string_table)?,
))),
```

### Tests

Update `src/compiler_frontend/declaration_syntax/tests/type_syntax_tests.rs` so collection expected values no longer include a `ValueMode`/ownership payload.

---

## Step 7 — Update expression construction

Edit:

```text
src/compiler_frontend/ast/expressions/expression.rs
```

### Struct field

Before:

```rust
pub ownership: Ownership,
```

After:

```rust
pub value_mode: ValueMode,
```

or if doing minimal field rename:

```rust
pub ownership: ValueMode,
```

The final Phase 3 check should prefer `value_mode`.

### Constructors

Update signatures:

```rust
pub fn new(
    kind: ExpressionKind,
    location: SourceLocation,
    data_type: DataType,
    value_mode: ValueMode,
) -> Self
```

```rust
fn scalar_literal(
    kind: ExpressionKind,
    data_type: DataType,
    location: SourceLocation,
    value_mode: ValueMode,
) -> Self
```

### Collection

Before:

```rust
DataType::Collection(Box::new(inner_type), ownership.to_owned())
```

After:

```rust
DataType::Collection(Box::new(inner_type))
```

The expression still stores `value_mode`.

### Struct instance

Before:

```rust
let struct_type = if const_record {
    DataType::const_struct_record(nominal_path, args.to_owned())
} else {
    DataType::runtime_struct(nominal_path, args.to_owned(), ownership.to_owned())
};
```

After:

```rust
let struct_type = if const_record {
    DataType::const_struct_record(nominal_path, args.to_owned())
} else {
    DataType::runtime_struct(nominal_path, args.to_owned())
};
```

### Copy

Before:

```rust
ownership.get_owned()
```

After:

```rust
value_mode.as_owned()
```

### Coercion

Before:

```rust
let ownership = value.ownership.to_owned();
...
ownership,
```

After:

```rust
let value_mode = value.value_mode.to_owned();
...
value_mode,
```

### Required search inside file

```bash
rg "Ownership|ownership|DataType::Collection\\(|runtime_struct" src/compiler_frontend/ast/expressions/expression.rs
```

Expected:

- no `Ownership`
- no collection ownership payload
- `ownership` only if field rename was intentionally deferred; preferred result is no lowercase `ownership` either

---

## Step 8 — Update AST node metadata

Edit:

```text
src/compiler_frontend/ast/ast_nodes.rs
```

### Imports

Replace `Ownership` import with `ValueMode`.

### `MultiBindTarget`

Before:

```rust
pub ownership: Ownership,
```

After:

```rust
pub value_mode: ValueMode,
```

### `NodeKind::FieldAccess`

Before:

```rust
ownership: Ownership,
```

After:

```rust
value_mode: ValueMode,
```

### `get_expr`

Before:

```rust
NodeKind::FieldAccess {
    data_type,
    ownership,
    ..
} => Ok(Expression::runtime(
    vec![self.to_owned()],
    data_type.to_owned(),
    self.location.to_owned(),
    ownership.to_owned(),
)),
```

After:

```rust
NodeKind::FieldAccess {
    data_type,
    value_mode,
    ..
} => Ok(Expression::runtime(
    vec![self.to_owned()],
    data_type.to_owned(),
    self.location.to_owned(),
    value_mode.to_owned(),
)),
```

For method/builtin calls, update:

```rust
ValueMode::MutableOwned
```

instead of `Ownership::MutableOwned`.

---

## Step 9 — Update declaration placeholders and builtins

Edit:

```text
src/compiler_frontend/headers/module_symbols.rs
```

### Function declaration placeholder

Before:

```rust
Expression::new(..., Ownership::ImmutableReference)
```

After:

```rust
Expression::new(..., ValueMode::ImmutableReference)
```

### Struct declaration placeholder

Before:

```rust
DataType::runtime_struct(
    header.tokens.src_path.to_owned(),
    fields.to_owned(),
    Ownership::MutableOwned,
)
```

After:

```rust
DataType::runtime_struct(
    header.tokens.src_path.to_owned(),
    fields.to_owned(),
)
```

Expression value mode remains:

```rust
ValueMode::ImmutableReference
```

### Start function return collection

Before:

```rust
DataType::Collection(
    Box::new(DataType::StringSlice),
    Ownership::MutableOwned,
)
```

After:

```rust
DataType::Collection(Box::new(DataType::StringSlice))
```

### Constant placeholder

Before:

```rust
let ownership = if declaration.mutable_marker {
    Ownership::MutableOwned
} else {
    Ownership::ImmutableOwned
};

declaration.to_data_type(&ownership)
```

After:

```rust
let value_mode = declaration.value_mode();

declaration.semantic_type()
```

Expression gets `value_mode`.

Edit:

```text
src/compiler_frontend/builtins/error_type.rs
```

Apply the same transformation:

- semantic struct/collection types contain no value mode
- expressions/declarations still carry `ValueMode`

---

## Step 10 — Update type resolution

Edit:

```text
src/compiler_frontend/ast/type_resolution.rs
src/compiler_frontend/ast/module_ast/pass_type_resolution.rs
```

### Struct construction

Replace:

```rust
DataType::runtime_struct(path, fields, Ownership::MutableOwned)
```

with:

```rust
DataType::runtime_struct(path, fields)
```

### Struct field resolution

Any pattern matching on:

```rust
DataType::Struct { ownership, .. }
```

must change to read only type identity:

```rust
DataType::Struct { nominal_path, fields, const_record }
```

If the code genuinely needs value mode, source it from the surrounding `Expression`, `Declaration`, or binding metadata.

### Constant resolution

Where constant declarations determine mutability:

- use `DeclarationSyntax::value_mode()`
- use `DeclarationSyntax::semantic_type()`

Do not reintroduce access state into `DataType`.

---

## Step 11 — Update field access, collection builtins, loops, mutation, call validation

These areas are likely to break during compilation and should be updated deliberately.

### Collection builtins

Edit:

```text
src/compiler_frontend/ast/field_access/collection_builtin.rs
```

Replace matches like:

```rust
DataType::Collection(inner, ownership)
```

with:

```rust
DataType::Collection(inner)
```

If a builtin needs mutability, derive it from:

- receiver expression `value_mode`
- call argument `CallAccessMode` / `CallPassingMode`
- binding declaration metadata
- not from the collection type

### Loops

Edit:

```text
src/compiler_frontend/ast/statements/loops.rs
```

Replace collection type construction and loop binding value modes.

Important invariant:

- loop item type is semantic element type
- loop binding mutability/access is local binding metadata

### Mutation

Edit files under:

```text
src/compiler_frontend/ast/expressions/mutation.rs
src/compiler_frontend/ast/statements/declarations.rs
src/compiler_frontend/ast/statements/multi_bind.rs
```

Replace `ownership` field names and enum names with `value_mode`.

Important invariant:

- assignment/mutation legality checks use value mode or mutable flags
- type compatibility checks use pure `DataType`

### Call validation

Edit:

```text
src/compiler_frontend/ast/expressions/call_validation.rs
```

Do not change call-site access rules. `CallAccessMode` and `CallPassingMode` already correctly separate call access intent from expression type.

Required invariant:

- function parameter type matching uses `DataType`
- mutable argument validation uses `CallAccessMode`, `CallPassingMode`, place/rvalue shape, and expression/local value mode

---

## Step 12 — Update type coercion

Edit:

```text
src/compiler_frontend/type_coercion/compatibility.rs
```

### Simplify struct compatibility

Current code has a struct special-case to ignore ownership:

```rust
if let (
    DataType::Struct {
        nominal_path: expected_path,
        const_record: expected_const_record,
        ..
    },
    DataType::Struct {
        nominal_path: actual_path,
        const_record: actual_const_record,
        ..
    },
) = (expected, actual)
{
    return expected_path == actual_path && expected_const_record == actual_const_record;
}
```

This can remain, but it should now be a clarity check rather than an ownership workaround.

Update the doc comment:

```rust
/// - Struct compatibility is nominal and const-record-sensitive.
/// - Collection compatibility is element-type compatibility only.
```

### Collection compatibility

If `expected == actual` is enough after the `DataType` rewrite, no special collection branch is needed.

If nested `Inferred` collection handling is needed, add explicit logic:

```rust
if let (DataType::Collection(expected_inner), DataType::Collection(actual_inner)) =
    (expected, actual)
{
    return is_type_compatible(expected_inner, actual_inner);
}
```

This is preferable if inferred collection element types are common in tests.

### Tests

Update compatibility tests added in Step 1.

---

## Step 13 — Update HIR type lowering

Edit:

```text
src/compiler_frontend/hir/hir_expression/types.rs
```

Before:

```rust
DataType::Collection(inner, _) => HirTypeKind::Collection {
    element: self.lower_data_type(inner, location)?,
},
```

After:

```rust
DataType::Collection(inner) => HirTypeKind::Collection {
    element: self.lower_data_type(inner, location)?,
},
```

Struct lowering becomes simpler if pattern already uses `..`:

```rust
DataType::Struct { nominal_path, .. } => ...
```

No HIR semantic change should happen.

---

## Step 14 — Update docs and comments

Required comment/doc updates:

### `datatypes.rs`

Add module doc saying no access/ownership state lives in `DataType`.

### `value_mode.rs`

Document that `ValueMode` is frontend value classification, not final runtime ownership metadata.

### `expression.rs`

Update struct doc:

```rust
pub struct Expression {
    pub data_type: DataType,
    pub value_mode: ValueMode,
    ...
}
```

Explain:

- `data_type` is semantic type
- `value_mode` is current frontend value/access classification

### `memory-management-design.md`

Only update if the docs currently imply `Ownership` is a frontend type property. Keep the core design intact.

### Search for stale wording

```bash
rg "ownership.*type|type.*ownership|Collection\\(.*Ownership|Struct.*ownership|DataType.*ownership" docs src
```

Any remaining wording must be either deleted or rewritten to distinguish runtime ownership from frontend `ValueMode`.

---

# Mandatory validation checks

## Static searches

Run these after implementation:

```bash
rg "Ownership" src
```

Expected: no code matches.

```bash
rg "DataType::Collection\\([^\\n]*,"
```

Expected: no matches for old two-argument collection construction.

```bash
rg "Collection\\(Box<DataType>,"
```

Expected: no matches.

```bash
rg "ownership:" src/compiler_frontend
```

Expected: no frontend AST/type metadata fields named `ownership`. If any remain, they must be runtime/backend-specific and documented as such.

```bash
rg "struct_ownership"
```

Expected: no matches.

```bash
rg "runtime_struct\\([^\\n]*Ownership|runtime_struct\\([^\\n]*ValueMode"
```

Expected: no matches.

## Compiler checks

Run:

```bash
cargo check
cargo test
```

Then:

```bash
cargo run tests
```

## Focused tests

Run focused tests for touched areas:

```bash
cargo test compatibility
cargo test type_syntax
cargo test constant_folding
cargo test hir_expression
cargo test hir_validation
cargo test borrow_checker
```

Adjust exact filters to existing test names where needed.

Run focused integration tags/cases for:

```bash
cargo run tests -- collections
cargo run tests -- structs
cargo run tests -- functions
cargo run tests -- borrows
cargo run tests -- type-checking
```

If the runner does not support these filters exactly, run the full integration runner.

## Full end-of-phase checks

Run:

```bash
cargo clippy
cargo run --features "detailed_timers" docs
cargo run --release --features "detailed_timers" speed-test.bst
```

Then run formatting last:

```bash
cargo fmt
```

---

# Required invariants after Phase 3

## Type model invariants

- [ ] `DataType::Collection` has exactly one payload: element type.
- [ ] `DataType::Struct` has no value/access/ownership payload.
- [ ] `DataType::PartialEq` does not compare value/access/ownership.
- [ ] `is_type_compatible` does not need to ignore ownership because ownership is not in the type.
- [ ] `DataType` docs explicitly state that access state belongs elsewhere.

## Value/access invariants

- [ ] AST expressions carry `ValueMode`.
- [ ] declarations or binding targets carry mutability/value mode where needed.
- [ ] function call arguments still use `CallAccessMode` and `CallPassingMode`.
- [ ] mutable call-site diagnostics still depend on explicit `~`.
- [ ] collection mutating methods still require explicit mutable receiver access.
- [ ] borrow checker behavior is unchanged.

## HIR invariants

- [ ] `HirTypeKind` remains pure type identity.
- [ ] `HirLocal.mutable` remains the local mutability channel.
- [ ] `lower_data_type` no longer discards collection ownership because there is none to discard.
- [ ] no ownership/access state is introduced into HIR type identity.

## Phase 2 preservation invariants

- [ ] `ModuleSymbols.declarations` remains the single sorted declaration placeholder source.
- [ ] no declaration-stub fallback is reintroduced.
- [ ] constant deferral still uses constant header paths or equivalent direct header-derived metadata.
- [ ] AST does not rebuild top-level declaration stubs.

---

# Risk analysis

## Risk 1 — Widespread mechanical breakage

### Cause

`Ownership` is used across many AST/frontend files.

### Mitigation

Use a mechanical rename first:

1. introduce `ValueMode`
2. replace enum name and method names
3. compile
4. remove `DataType` payloads

Do not combine this with semantic behavior changes.

---

## Risk 2 — Collection inference breaks

### Cause

`DeclarationSyntax::to_data_type` currently rewrites collection ownership based on declaration mutability. Removing this may expose places that were relying on collection ownership as a proxy for mutability.

### Mitigation

Move that logic to `ValueMode`:

```rust
DeclarationSyntax::value_mode()
```

Then update declaration resolution to use:

- `semantic_type()` for type checking
- `value_mode()` for binding metadata

Add integration coverage for mutable and immutable collection declarations.

---

## Risk 3 — Struct mutation behavior changes

### Cause

Some code may have been reading `DataType::Struct { ownership }` to decide field mutation legality.

### Mitigation

Any such logic must instead use the expression/declaration `ValueMode`.

Required check:

```bash
rg "DataType::Struct \\{[^}]*ownership|struct_ownership"
```

Expected no matches.

---

## Risk 4 — Type compatibility gets too permissive

### Cause

Removing ownership from type equality means some formerly distinct types compare equal.

### This is intended.

However, mutability/access restrictions must still be enforced outside type compatibility.

### Mitigation

Ensure these remain rejected:

- mutation through immutable binding
- calling mutable parameter without `~`
- using `~` on immutable place
- using `~` on non-place expression
- overlapping mutable/shared borrow

These already have integration fixtures; add missing ones if needed.

---

## Risk 5 — HIR lowering loses necessary information

### Cause

If some backend/lowering path implicitly depended on `DataType` ownership payload, removing it may appear to remove data.

### Mitigation

HIR already models type identity without ownership. Access/borrow info should flow through:

- AST expression `ValueMode`
- call argument `CallPassingMode`
- HIR local `mutable`
- borrow checker facts
- later ownership/drop analysis

No HIR type should gain ownership state to compensate.

---

# Required implementation checklist

## Before code changes

- [ ] Phase 2 searches for declaration stubs are clean.
- [ ] Add/confirm type/access separation tests.
- [ ] Add/confirm mutable/immutable collection and struct integration coverage.

## During code changes

- [ ] Add `ValueMode`.
- [ ] Move frontend value metadata from `Ownership` to `ValueMode`.
- [ ] Remove `Ownership` enum from `datatypes.rs`.
- [ ] Remove `Ownership` imports from `datatypes` call sites.
- [ ] Remove `Ownership` from `DataType::Collection`.
- [ ] Remove `ownership` from `DataType::Struct`.
- [ ] Remove `struct_ownership`.
- [ ] Replace `DeclarationSyntax::to_data_type(&Ownership)` with `semantic_type()` and `value_mode()`.
- [ ] Update type syntax collection parsing.
- [ ] Update named-type traversal and resolution.
- [ ] Update expression constructors.
- [ ] Update AST nodes and field-access metadata.
- [ ] Update module symbol placeholder construction.
- [ ] Update builtin type construction.
- [ ] Update AST type resolution.
- [ ] Update collection builtin handling.
- [ ] Update loop binding handling.
- [ ] Update mutation/declaration/multi-bind handling.
- [ ] Update HIR type lowering.
- [ ] Update comments and docs.

## After code changes

- [ ] `rg "Ownership" src` returns no code matches.
- [ ] `rg "DataType::Collection\\([^\\n]*," src` returns no old two-argument constructors.
- [ ] `rg "struct_ownership" src` returns no matches.
- [ ] `cargo check` passes.
- [ ] `cargo test` passes.
- [ ] `cargo run tests` passes.
- [ ] `cargo clippy` passes.
- [ ] docs build passes.
- [ ] speed test still compiles.

---

# Expected final state

After Phase 3, the frontend should have a sharper model:

```text
DataType
  = semantic type identity only

ValueMode
  = frontend expression/binding value classification

CallAccessMode / CallPassingMode
  = call-site access intent and validated passing form

HIR TypeId / HirTypeKind
  = canonical backend-facing type identity

HIR locals + borrow facts
  = mutability/exclusivity/borrow analysis state

Future ownership lowering
  = optional backend/runtime optimization state
```

This makes the compiler design more consistent:

- Parser/type syntax produces pure types.
- Declaration syntax separately records mutability.
- AST expressions separately carry current value/access mode.
- Type compatibility checks only types.
- Borrow checking owns access/exclusivity validation.
- Runtime ownership remains a later lowering concern, not a static type property.
