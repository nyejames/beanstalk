# Beanstalk Traits

Traits define a named behavioural contract for a type.

They are intended to be simple, explicit, and cheap for the compiler to resolve.
Beanstalk traits are designed to support static method resolution by default without dragging the language toward Rust-level trait-system complexity or runtime interface-object semantics.

Style note: trait names will always be fully capitalised. They will be the only language construct to do this to help them stand out.

## Goals

- Keep traits **nominal** and **explicit**.
- Keep method lookup **statically resolved by default**.
- Avoid implicit structural conformance.
- Avoid runtime trait objects in v1.
- Preserve compile speed and keep the implementation tractable.
- Leave room for later generic constraints and optional optimisations.

## Non-goals for v1

Traits are **not** intended to be:

- runtime interface objects
- implicit duck typing
- inheritance
- a full Rust-style trait solver
- a source of large compile-time or code-size blowups

The first version should stay intentionally narrow.

## Syntax

Trait declarations use the keyword `must`:

```beanstalk
DRAWABLE must:
    draw |This, surface Surface| -> String;
    bounds |This| -> Rect, Bool;
;
```

This reads as: any type claiming to satisfy `DRAWABLE` **must** provide these methods.

## The `This` placeholder

Inside a trait declaration, `This` refers to the concrete implementing type.

```beanstalk
Resizable must:
    resize |~This, width Int, height Int|;
;
```

Rules:

- `This` means an immutable receiver requirement.
- `~This` means a mutable receiver requirement.
- `This` is trait-local syntax and does not name a real user-defined type.
- Trait signatures describe required methods only. They do not provide storage or fields.

This keeps trait method requirements aligned with Beanstalk's receiver-based method model while avoiding the awkwardness of pretending a trait signature has a literal `this` parameter name.

## Semantics

Traits are intended to follow these rules:

- **Nominal**: a type satisfies a trait only through an explicit implementation.
- **Explicit**: conformance is declared deliberately, never inferred from matching method shapes.
- **Statically resolved by default**: concrete receiver calls resolve directly to known methods.
- **No runtime trait objects in v1**: traits do not imply boxed dynamic values, vtables, or subtype-style object polymorphism.
- **No implicit structural matching**: similar method signatures alone do not make two types interchangeable.

This fits Beanstalk better than Go-style interfaces or TypeScript-style structural interfaces.

## Implementation model

The exact implementation syntax can be finalised separately, but the model should remain:

- explicit
- nominal
- local and readable
- checked against the declared trait surface

The compiler should verify that every required method exists with the correct receiver mutability, parameter types, and return signature.

## Dispatch and compile-time strategy

A major design concern for traits is avoiding compile-time explosions.

Rust-style trait systems become expensive mostly when several features stack together:

- heavy monomorphization
- deep trait solving
- blanket impls
- specialization
- associated types
- overlapping or highly generic trait relationships

Beanstalk should avoid this by keeping the first trait system deliberately simple.

### Preferred strategy

#### Concrete calls

When the receiver type is known, trait methods should resolve directly at compile time.

```beanstalk
shape.draw(surface)
```

If `shape` is known to be a `Circle`, this should lower directly to the concrete method implementation.

#### Generic trait-bounded code

If trait-bounded generics are added, Beanstalk should prefer a **shared-body** strategy by default rather than mandatory monomorphization.

That means generic functions can be compiled once and receive trait method information through an internal witness/dictionary-style mechanism.

This trades some runtime performance for:

- more stable compile times
- less duplicated generated code
- smaller code size
- a simpler backend model

Selective monomorphization can still be added later as an optimisation, especially for release builds, but it should not define the semantic model of traits.

## V1 scope

Recommended first slice:

- trait declarations with `must`
- explicit implementations
- required methods only
- `This` / `~This` receiver requirements
- static resolution for concrete calls
- structured diagnostics for unsupported or invalid trait usage

Traits should **not** initially include:

- runtime trait objects
- default methods
- trait inheritance
- blanket implementations
- specialization
- associated types
- implicit conformance

Default methods can be added later if they still fit the language cleanly.

## Diagnostics

Unsupported trait syntax or partially implemented trait features should produce intentional, structured diagnostics.

Examples:

- trait declared correctly but full implementation not yet supported
- invalid `This` usage outside trait declarations
- missing required method in a trait implementation
- wrong receiver mutability (`This` vs `~This`)
- method signature mismatch

Traits should never fail through parser ambiguity or accidental fallback into unrelated constructs.

## Design tradeoffs

This design deliberately chooses:

- simpler rules over maximal flexibility
- explicit conformance over implicit convenience
- predictable compile times over aggressive zero-cost abstraction by default
- a narrow feature slice now so the language can grow without syntax churn later

The result is a trait system that matches Beanstalk's broader design priorities:

- readability first
- strong, intentional syntax
- static correctness
- implementation tractability
- room for future optimisation without changing source semantics

## Summary

Traits in Beanstalk are behavioural contracts written with `must`.

They are intended to be:

- nominal
- explicit
- statically resolved by default
- non-structural
- non-object-based in v1

This keeps the feature aligned with the language's existing method model while avoiding the complexity and compile-time cost of a more ambitious trait system too early.