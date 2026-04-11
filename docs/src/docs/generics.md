# Beanstalk Generics Design (Draft)

This is intended as a future reference for language design and compiler implementation planning. It is not final user-facing documentation yet.

## Status

Draft design for future alpha-stage implementation.

## Summary

Beanstalk should support a **small, explicit, keyword-based generics system**.

The initial design should focus on:

- generic functions
- generic structs
- generic choices / sum types
- type inference where obvious
- trait-constrained generic parameters later

The design should **not** attempt to match Rust, C++, or TypeScript in power or complexity.

The goal is to add the useful part of generics without undermining Beanstalk’s existing design principles around readability, explicitness, compile speed, and syntax consistency.

## Why generics are needed

Traits and type unions are useful, but they do not replace generics.

They solve different problems:

- **Traits** describe required behaviour.
- **Type unions** describe a closed set of possible concrete types.
- **Generics** describe a type relationship where one or more concrete types are chosen by the caller and preserved through the API.

That last category is the important gap.

Generics are needed for cases such as:

- returning the same type that was passed in
- expressing that two parameters must be the same concrete type
- reusable typed containers such as `Box`, `Option`, `Result`, `Array`, `Map`, or `Set`
- reusable algorithms that preserve strong static typing
- avoiding code duplication across many concrete types
- avoiding weaker “accept anything” APIs that lose type precision

Without generics, Beanstalk would eventually be pushed toward one of these poor outcomes:

- compiler-owned special cases for too many library patterns
- duplicated implementations for many concrete types
- weaker typing in reusable APIs

## Design goals

The generic system should follow the existing language style.

### Goals

- Keep syntax readable and keyword-based.
- Avoid punctuation-heavy or symbolic syntax.
- Preserve the “one concept, one signal” direction of the language.
- Keep generic declaration and generic application visually distinct.
- Support strong type inference where it is obvious and predictable.
- Keep implementation tractable in the frontend and type checker.
- Leave room for trait constraints later without forcing a redesign.
- Avoid introducing runtime generic machinery as a semantic requirement.

### Non-goals for the initial version

The first implementation should not include:

- higher-kinded types
- const generics
- specialization
- lifetime parameters
- generic associated types
- advanced trait solving
- complex implicit inference rules
- dynamic generic reification as a language feature

## Core syntax decision

The recommended syntax is:

- **`type`** for declaring generic parameters
- **`of`** for applying generic arguments

This gives Beanstalk a clean and readable generic system without reusing symbols that already carry strong meaning elsewhere in the language.

## Generic declaration syntax

Generic parameters are declared directly after the function, struct, or choice name using the `type` keyword.

### Functions

```bst
identity type T |value T| -> T:
    return value
;
```

```bst
pair_first type A, B |left A, right B| -> A:
    return left
;
```

### Structs

```bst
Box type T = |
    value T,
|
```

```bst
Pair type A, B = |
    left A,
    right B,
|
```

### Choices

```bst
Option type T ::
    Some T,
    None,
;
```

```bst
Result type T, E ::
    Ok T,
    Err E,
;
```

## Generic application syntax

Generic arguments are applied using `of`.

### Type positions

```bst
value Box of Int = Box(42)
result Result of String, Error = Result.Ok("hello")
maybe_name Option of String = Option.Some("Nye")
```

This gives Beanstalk a readable type-application form that scales well:

```bst
Array of String
Map of String, Int
Result of Page, Error
Signal of User
```

## Function calls and inference

Generic functions should rely on **type inference by default**.

That means the common case should look like this:

```bst
identity type T |value T| -> T:
    return value
;

number = identity(42)
text = identity("hello")
```

The compiler infers `T` from the call arguments.

This keeps generic functions lightweight in normal code and avoids noise.

## Explicit generic function application

Explicit generic function application may be supported later if needed:

```bst
value = identity of Int(42)
```

However, this does **not** need to be required in the first implementation.

### Recommended alpha approach

For the initial version:

- support generic functions
- infer generic arguments from value arguments where possible
- support explicit `of` in type positions immediately
- defer explicit generic call-site application unless implementation pressure makes it necessary

This keeps the first implementation smaller and avoids settling parser edge cases too early.

## Why this syntax fits Beanstalk

### 1. It is readable

`identity type T` and `Box of Int` are both easy to read.

They look like authored language syntax rather than imported generic punctuation from another language.

### 2. It preserves syntax ownership

Beanstalk already assigns strong meaning to its core punctuation and keywords.

Using keyword-based generics avoids overloading symbols that already have a stable role.

### 3. It avoids angle bracket problems

A classic `<T>` generic syntax is intentionally avoided.

Reasons:

- it conflicts with Beanstalk’s syntax philosophy
- it introduces punctuation-heavy parsing and nested readability issues
- it creates pressure around future type-grammar extensions
- it risks ambiguity and uglier parsing rules in comparison-heavy contexts

### 4. It scales across declaration kinds

The same declaration pattern works for:

- functions
- structs
- choices

That consistency matters.

### 5. It leaves future constraint syntax open

By using `type` only for parameter introduction and `of` only for application, future constraints can be added later as a separate clause.

For example:

```bst
sort type T |items {T}| -> {T}
where T is ORDERED:
    ...
;
```

Or:

```bst
sort type T is ORDERED |items {T}| -> {T}:
    ...
;
```

This should remain undecided until trait design is more settled.

## Why traits and unions are not enough

Traits and unions remain useful, but they do not replace generics.

### Traits

Traits answer:

> What behaviour must this type support?

Example direction:

```bst
sort type T ...
where T is ORDERED
```

That describes required behaviour, but it still needs a generic parameter to preserve the actual concrete type through the API.

### Type unions

Type unions answer:

> Which closed set of concrete types is accepted here?

That is useful for intentionally closed APIs.

But unions do not express:

- the output is the same concrete type as the input
- both parameters must be the same concrete type
- a container stores some caller-chosen type `T`

That is the core role of generics.

## Relationship to Beanstalk’s implementation goals

The generic system should stay aligned with the broader language and compiler direction.

### Important implementation principles

- Generics should be primarily a **frontend type-system feature**.
- They should not force a large semantic rewrite of borrowing or ownership.
- They should keep static resolution as the default direction.
- They should not require runtime interface-object semantics.
- They should not grow into a large meta-programming system.

Beanstalk’s current design already emphasizes readability, strong static typing, compile speed, and backend flexibility. The generic system should reinforce those goals rather than complicate them.

## Suggested initial language rules

These are the recommended initial rules for a first implementation.

### Declaration rules

- Generic parameters are introduced with `type`.
- Multiple parameters are comma-separated.
- Generic parameters are scoped to the declaration they belong to.
- Generic parameter names follow normal type-style naming conventions.

### Use-site rules

- Generic type application uses `of`.
- Multiple generic arguments are comma-separated.
- Generic type application is valid in normal type positions.
- Nested generic types should be readable and unambiguous.

### Inference rules

- Generic function calls should infer arguments from normal value arguments where possible.
- Inference should remain predictable and conservative.
- If inference fails, the compiler should emit a clear diagnostic.
- Explicit generic call-site syntax can be added later if needed.

### Error handling expectations

Diagnostics should be explicit and structured.

Examples of likely user-facing failures:

- wrong number of generic arguments
- generic argument provided where none are expected
- missing generic arguments in a type position where they are required
- inference failure for a generic function call
- use of a non-type symbol where a type argument is expected
- use of constraint syntax before it is implemented

## Examples

### Identity function

```bst
identity type T |value T| -> T:
    return value
;

x = identity(42)
y = identity("hello")
```

### Generic struct

```bst
Box type T = |
    value T,
|

number_box Box of Int = Box(42)
text_box Box of String = Box([: hello ])
```

### Generic choice

```bst
Result type T, E ::
    Ok T,
    Err E,
;

parse_name || -> Result of String, Error:
    return Result.Ok("Nye")
;
```

### Relationship-preserving API

```bst
first type T |items {T}| -> T:
    return items.get(0)!
;
```

This is exactly the kind of API that traits or unions do not express cleanly by themselves.

## Syntax alternatives considered

### Angle brackets

```bst
Box<Int>
identity<Int>(42)
```

Rejected.

Reason:

- does not fit Beanstalk’s syntax philosophy
- visually noisier
- creates future grammar pressure
- weakens syntax ownership

### Symbol-based forms such as `&`

Rejected.

Reason:

- cryptic
- visually dense
- hard to scale cleanly
- easy to regret later

### Treating type parameters like normal parameters

Example direction:

```bst
identity |T Type, value T| -> T:
```

Rejected for now.

Reason:

- blurs value parameters and type parameters too much
- makes generic parameters feel like runtime arguments
- is less clean across structs and choices
- introduces more magic into parameter parsing

## Future extension points

The syntax is designed to leave room for later features.

Possible future additions:

- trait constraints
- generic methods where appropriate
- explicit generic call-site application
- standard library generic containers
- compiler diagnostics tailored for inference and constraints

Possible future constraint directions:

```bst
sort type T |items {T}| -> {T}
where T is ORDERED:
    ...
;
```

```bst
contains type T |items {T}, value T| -> Bool
where T is EQUALS:
    ...
;
```

These should be designed later, once the trait model is more stable.

## Recommended alpha scope

The initial implementation should stay narrow.

### Include

- generic functions
- generic structs
- generic choices
- `type` declaration syntax
- `of` type application syntax
- simple predictable inference for function calls

### Defer

- advanced constraints
- explicit generic call-site application unless necessary
- advanced inference
- generic methods if they complicate the implementation
- any feature that pushes Beanstalk toward Rust-level generic complexity

## Final direction

Beanstalk should adopt a **minimal, keyword-based generic system** built around:

- `type` for generic parameter declaration
- `of` for generic argument application

This is the cleanest known direction for the language at this stage because it is:

- readable
- consistent
- future-friendly
- easy to teach
- aligned with the language’s syntax philosophy
- restrained enough for an alpha-stage implementation

The main principle is simple:

> Beanstalk should support the useful 80% of generics without inheriting the full complexity of larger generic systems.