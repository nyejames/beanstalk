#import(@libs/html/Basic)
#import(@styles/Docs)
#import(@./components)
[Basic.page:
[Docs.Navbar]

[Docs.header Basic.center: [Docs.title(1): MEMORY MANAGEMENT]]
[Basic.page:

[Basic.red: Memory management scheme still in testing / design phase ]

# Current ideas about memory management
The intention is for Beanstalk to *not* have a garbage collector.

There will be need to be safety and avoidance of common bugs without a significant penalty to how optimized the output can be.

The current plan is to have a system similar to Rust, 
but due to the language design, the borrow checking process will be much simpler and faster.

The goal is also to have the same level of precision as Polonius, or at least a stronger version of NLL.
This is possible through restrictions in the language and avoiding having to evaluate unsafe edge cases entirely.

There will be no distinct lifetime annotation syntax. 
You can only return references to parameters. 
Return references from functions by using the parameter name in the return signature instead of just a type.
This will mean "return a reference with the same lifetime and type as this parameter".
All other lifetimes will be elided by the compiler.

Instead of explicitly annotating when values are moved, 
Beanstalk defaults to reference semantics and moves are statically determined by the compiler.

Due to reference semantics, the language does not support creating temporaries.

This means you only need to worry about whether you are providing a mutable reference, immutable reference or a copy of a value.

# Development notes
struct Place {
    root: PlaceRoot,
    projections: Vec<Projection>,
}
enum PlaceRoot {
    Local(LocalId),
    Param(ParamId),
    Global(GlobalId),
}

Notes:
- Temporary does not exist in the language semantics #
(They may be needed in the IR, but for now we will try to just use locals)
- No heap object roots exposed to the language
- References always point to a root + projections

A Beanstalk Place is:

A root + a projection path

And nothing more.

Because the language forbids:
- Raw pointers
- Unsafe
- Closures
- Interior mutability
- Explicit moves

enum Projection {
    Field(FieldIndex),        // struct fields
    Index(IndexKind),         // arrays / slices
    Deref,                    // reference dereference
}
enum IndexKind {
    Constant(u32),            // arr[3]
    Dynamic,                  // arr[i]
}

Important restriction:
- Deref may only appear if the type is &T or ~T
- No chained pointer arithmetic
- No *&*x madness

## Overlap rules (much simpler than Rust)

With this model, overlap is a pure structural check.

Two places overlap if:
- They share the same root
- One projection list is a prefix of the other

x        overlaps x.a
x.a      overlaps x.a.b
x.a      does NOT overlap x.b

## Borrows
struct Borrow {
    id: BorrowId,
    place: Place,
    kind: BorrowKind, // Shared | Mutable
}
And track:
- where the borrow is created
- where it is last used
- which CFG paths it flows through

## Additional Restructions
- Forbid borrowing the “whole” if parts are borrowed
let r = &x.field;
let s = &x;   -- error

- Functions can only return references to parameters, never to locals or globals (enforced at language level)
- Temporaries cannot be created in the language semantics 

]

