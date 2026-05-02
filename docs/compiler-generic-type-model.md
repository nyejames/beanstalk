# Compiler Generic Type Model

This document defines the generic-ready type-system substrate introduced in Phase 0.

Phase 0 does **not** implement user-visible generic syntax.

Phase 0 only adds compiler infrastructure for generic declarations and generic type references.

## Core model

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

## Data contracts

The frontend type model is extended with structural generic variants:

```rust
DataType::TypeParameter { id, name }
DataType::GenericInstance { base, arguments }
```

Generic base identity is structural:

```rust
GenericBaseType::Named(StringId)
GenericBaseType::ResolvedNominal(InternedPath)
GenericBaseType::External(ExternalTypeId)
GenericBaseType::Builtin(BuiltinGenericType)
```

`BuiltinGenericType` currently includes `Collection` only.

## Type identity keys

Generic identity never uses stringified names.

```rust
GenericInstantiationKey {
    base_path: InternedPath,
    arguments: Vec<TypeIdentityKey>,
}

TypeIdentityKey {
    Builtin(BuiltinTypeKey),
    Nominal(InternedPath),
    External(ExternalTypeId),
    Collection(Box<TypeIdentityKey>),
    Option(Box<TypeIdentityKey>),
    Result { ok, err },
    GenericInstance(GenericInstantiationKey),
}
```

## Type substitution

`TypeSubstitution` maps `TypeParameterId -> DataType` and substitutes recursively through:

- `TypeParameter`
- `GenericInstance`
- `Collection`
- `Option`
- `Result`
- `Reference`
- `Returns`
- `Function`
- `Struct` field types
- `Choices` payload field types
- `Parameters`

## Type resolution ordering

Shared resolution logic resolves names in this order:

1. Declaration-local generic parameters
2. Visible type aliases
3. Visible source declarations
4. Visible external types
5. Builtins

## Locked Phase 0 decisions

| Question | Decision |
|---|---|
| Are generic parameters types only? | Yes. No const generics. No lifetime generics. |
| Are trait bounds part of Phase 0? | No. Traits are deferred. |
| Are generics nominal or structural? | Generic structs/choices remain nominal after instantiation. |
| Are generic aliases transparent? | Concrete generic aliases are transparent. Generic aliases are deferred. |
| Are `Option` / error results ordinary generic choices? | No. They keep distinct semantics. |
| Are generic functions monomorphized immediately? | Backend strategy is not encoded in this model. |
| Are collections ordinary public generics? | No. Public syntax remains `{T}`. |
| Is collection capacity a type argument? | No. Capacity is allocation metadata, not type identity. |

## HIR contract

Executable HIR remains concrete-only.

Frontend metadata may track generic declarations, but unresolved generic parameters must not appear in executable HIR.

## Shared lowering rule

Choices, Options, and Results may share payload/variant carrier lowering machinery.

They must not be forced to share all semantic rules:

- Choices keep tag/exhaustiveness semantics.
- Options keep contextual `none` semantics.
- Results keep propagation/error-channel semantics.
