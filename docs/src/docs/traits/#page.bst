import @html {center}
import @styles/docs {navbar, title, section, styled_table, header, data, codeblock, codesnippet, theme_head}

# page_title = "Traits in Beanstalk"
# page_description = "Design draft for nominal, explicit, statically resolved traits in Beanstalk."
# page_head = theme_head

#[navbar]
#[title, center: TRAITS IN BEANSTALK]

#[section, $markdown:
    **Design draft:** This page is a future-facing design document for language and compiler planning.
    It is not final user-facing documentation, and the syntax, semantics, and implementation details may change before this feature is implemented.

    Traits define a named behavioural contract for a type.

    They are intended to be simple, explicit, and cheap for the compiler to resolve.

    Beanstalk traits are designed to support static method resolution by default without pushing the language toward Rust-level trait-system complexity or runtime interface-object semantics.

    Current compiler status:

    - traits are deferred for Alpha
    - [codesnippet:must] is reserved
    - [codesnippet:This] is reserved
    - both reserved tokens should reject with structured diagnostics when used incorrectly

    This page is a design reference, not current compiler support.
]

#[section, $markdown:
    # Design goals

    Traits in Beanstalk are designed to:

    - Keep traits nominal and explicit
    - Keep method lookup statically resolved by default
    - Avoid implicit structural conformance
    - Avoid runtime trait objects in v1
    - Preserve compile speed
    - Keep the implementation tractable
    - Leave room for later generic constraints and optional optimisations

    Trait names should be fully capitalised.

    Traits are the only planned language construct that uses this naming convention.

    This makes trait contracts visually distinct from types, choices, structs, functions, and variables.
]

#[section, $markdown:
    # Non-goals for v1

    Traits are not intended to be:

    - runtime interface objects
    - implicit duck typing
    - inheritance
    - a full Rust-style trait solver
    - a source of large compile-time or code-size blowups

    The first version should stay intentionally narrow.

    The goal is a small trait system that fits Beanstalk's receiver method model, not a maximal abstraction system.
]

#[section, $markdown:
    # Syntax

    Trait declarations use [codesnippet:must].

    [codeblock, $code("bst"):
        DRAWABLE must:
            draw |This, surface Surface| -> String;
            bounds |This| -> Rect, Bool;
        ;
    ]

    This reads as:

    > Any type claiming to satisfy [codesnippet:DRAWABLE] must provide these methods.

    A trait declaration contains required method signatures.

    It does not provide fields, storage, or default behavior in v1.
]

#[section, $markdown:
    # The This placeholder

    Inside a trait declaration, [codesnippet:This] refers to the concrete implementing type.

    [codeblock, $code("bst"):
        RESIZABLE must:
            resize |~This, width Int, height Int|;
        ;
    ]

    Rules:

    - [codesnippet:This] means an immutable receiver requirement
    - [codesnippet:~This] means a mutable receiver requirement
    - [codesnippet:This] is trait-local syntax
    - [codesnippet:This] does not name a real user-defined type
    - trait signatures describe required methods only
    - trait signatures do not provide storage or fields

    This keeps trait method requirements aligned with Beanstalk's receiver-based method model.

    It also avoids pretending that a trait signature has a literal [codesnippet:this] parameter name.
]

#[section, $markdown:
    # Semantics

    Traits are intended to follow a small set of rules.

    [styled_table:
        [header: [: Rule] [: Meaning] ]

        [data:
            [: Nominal ]
            [: A type satisfies a trait only through an explicit implementation. ]
        ]

        [data:
            [: Explicit ]
            [: Conformance is declared deliberately. It is never inferred from matching method shapes. ]
        ]

        [data:
            [: Statically resolved by default ]
            [: Concrete receiver calls resolve directly to known methods. ]
        ]

        [data:
            [: No runtime trait objects in v1 ]
            [: Traits do not imply boxed dynamic values, vtables, or subtype-style object polymorphism. ]
        ]

        [data:
            [: No implicit structural matching ]
            [: Similar method signatures alone do not make two types interchangeable. ]
        ]
    ]

    This fits Beanstalk better than Go-style interfaces or TypeScript-style structural interfaces.
]

#[section, $markdown:
    # Implementation model

    The exact implementation syntax can be finalised separately.

    The model should remain:

    - explicit
    - nominal
    - local and readable
    - checked against the declared trait surface

    The compiler should verify that every required method exists with the correct:

    - receiver mutability
    - parameter types
    - return signature

    A type should not satisfy a trait accidentally.

    Conformance should be something the programmer writes deliberately and the compiler verifies directly.
]

#[section, $markdown:
    # Concrete dispatch

    When the receiver type is known, trait methods should resolve directly at compile time.

    [codeblock, $code("bst"):
        shape.draw(surface)
    ]

    If [codesnippet:shape] is known to be a [codesnippet:Circle], this should lower directly to the concrete [codesnippet:Circle] method implementation.

    This keeps ordinary method calls cheap and predictable.

    Traits should not add dynamic dispatch overhead to concrete calls.
]

#[section, $markdown:
    # Generic trait-bounded code

    If trait-bounded generics are added, Beanstalk should prefer a shared-body strategy by default rather than mandatory monomorphization.

    That means a generic function can be compiled once and receive trait method information through an internal witness or dictionary-style mechanism.

    This trades some runtime performance for:

    - more stable compile times
    - less duplicated generated code
    - smaller code size
    - a simpler backend model

    Selective monomorphization can still be added later as a release-build optimisation.

    It should not define the semantic model of traits.
]

#[section, $markdown:
    # Why avoid a Rust-style trait system?

    Rust-style trait systems become expensive mostly when several powerful features stack together:

    - heavy monomorphization
    - deep trait solving
    - blanket implementations
    - specialization
    - associated types
    - overlapping or highly generic trait relationships

    Beanstalk should avoid this in the first trait design.

    The first version should solve one problem well:

    > A type can explicitly promise that it provides a named set of methods, and the compiler can check that promise.
]

#[section, $markdown:
    # Recommended v1 scope

    The first implementation should include:

    - trait declarations with [codesnippet:must]
    - explicit implementations
    - required methods only
    - [codesnippet:This] and [codesnippet:~This] receiver requirements
    - static resolution for concrete calls
    - structured diagnostics for unsupported or invalid trait usage

    Traits should not initially include:

    - runtime trait objects
    - default methods
    - trait inheritance
    - blanket implementations
    - specialization
    - associated types
    - implicit conformance

    Default methods can be added later if they still fit the language cleanly.
]

#[section, $markdown:
    # Diagnostics

    Unsupported trait syntax or partially implemented trait features should produce intentional, structured diagnostics.

    Useful diagnostic cases include:

    - trait declared correctly, but full implementation is not yet supported
    - invalid [codesnippet:This] usage outside trait declarations
    - missing required method in a trait implementation
    - wrong receiver mutability
    - method parameter mismatch
    - method return signature mismatch
    - trait name not using the fully capitalised convention

    Traits should never fail through parser ambiguity or accidental fallback into unrelated constructs.
]

#[section, $markdown:
    # Invalid This usage

    [codesnippet:This] is valid only inside trait declarations.

    Invalid:

    [codeblock, $code("bst"):
        value This = something

        function |input This|:
            io(input)
        ;
    ]

    The compiler should reject this clearly.

    Example diagnostic direction:

    [codeblock, $code("text"):
        'This' is only valid inside trait declarations.
        Use a concrete type name here.
    ]
]

#[section, $markdown:
    # Receiver mutability

    Trait method requirements must distinguish immutable and mutable receivers.

    Immutable receiver requirement:

    [codeblock, $code("bst"):
        DISPLAY must:
            display |This| -> String;
        ;
    ]

    Mutable receiver requirement:

    [codeblock, $code("bst"):
        RESETTABLE must:
            reset |~This|;
        ;
    ]

    A method that only accepts [codesnippet:This] should not satisfy a [codesnippet:~This] requirement.

    A method that requires [codesnippet:~This] should not satisfy an immutable [codesnippet:This] requirement.

    Receiver access is part of the contract.
]

#[section, $markdown:
    # Relationship to methods

    Traits should build on Beanstalk's existing receiver method model.

    Regular methods are already top-level functions with a receiver-style first parameter.

    Example concrete method:

    [codeblock, $code("bst"):
        Circle = |
            radius Float,
        |

        draw |this Circle, surface Surface| -> String:
            return [:Drawing circle]
        ;
    ]

    A trait requirement describes the required shape without naming a concrete type:

    [codeblock, $code("bst"):
        DRAWABLE must:
            draw |This, surface Surface| -> String;
        ;
    ]

    The concrete method and the trait requirement should line up by receiver type, parameter list, and return list.
]

#[section, $markdown:
    # Design tradeoffs

    This design deliberately chooses:

    - simpler rules over maximal flexibility
    - explicit conformance over implicit convenience
    - predictable compile times over aggressive zero-cost abstraction by default
    - a narrow feature slice now so the language can grow without syntax churn later

    This matches Beanstalk's broader design priorities:

    - readability first
    - strong, intentional syntax
    - static correctness
    - implementation tractability
    - room for future optimisation without changing source semantics
]

#[section, $markdown:
    # Deferred features

    These features should be deferred until the core trait model is stable:

    - runtime trait objects
    - default method bodies
    - trait inheritance
    - associated types
    - specialization
    - blanket implementations
    - conditional implementations
    - negative implementations
    - structural conformance
    - automatic deriving
    - dynamic dispatch syntax

    These are powerful features, but adding them too early would make traits a compiler complexity trap.

    The first version should stay deliberately boring.
]

#[section, $markdown:
    # Summary

    Traits in Beanstalk are behavioural contracts written with [codesnippet:must].

    They are intended to be:

    - nominal
    - explicit
    - statically resolved by default
    - non-structural
    - non-object-based in v1

    The core syntax is:

    [codeblock, $code("bst"):
        DRAWABLE must:
            draw |This, surface Surface| -> String;
        ;
    ]

    The core rule is:

    > A trait is a named promise that a type provides a required method surface.

    This keeps traits aligned with Beanstalk's existing method model while avoiding the complexity and compile-time cost of a more ambitious trait system too early.
]