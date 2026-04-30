import @html {center}
import @styles/docs {navbar, title, section, styled_table, header, data, codeblock, codesnippet, theme_head}

# page_title = "Generics in Beanstalk"
# page_description = "Design draft for a small, explicit, keyword-based generics system in Beanstalk."
# page_head = theme_head

#[navbar]
#[title, center: GENERICS IN BEANSTALK]

#[section, $markdown:
    **Design draft:** This page is a future-facing design document for language and compiler planning.
    It is not final user-facing documentation, and the syntax, semantics, and implementation details may change before this feature is implemented.

    Beanstalk should support a small, explicit, keyword-based generics system.

    The initial design should focus on:

    - generic functions
    - generic structs
    - generic choices
    - type inference where obvious
    - trait-constrained generic parameters later

    The design should not attempt to match Rust, C++, or TypeScript in power or complexity.

    The goal is to add the useful part of generics without undermining Beanstalk's existing design principles around readability, explicitness, compile speed, and syntax consistency.
]

#[section, $markdown:
    # Status

    Generics are a draft design for future alpha-stage implementation.

    The recommended direction is:

    [styled_table:
        [header: [: Concept] [: Syntax] [: Meaning] ]

        [data:
            [: Generic parameter declaration ]
            [: [codesnippet:type] ]
            [: Introduces one or more type parameters. ]
        ]

        [data:
            [: Generic argument application ]
            [: [codesnippet:of] ]
            [: Applies concrete type arguments to a generic type. ]
        ]

        [data:
            [: Function inference ]
            [: inferred from call arguments ]
            [: Generic function calls should usually not need explicit type arguments. ]
        ]

        [data:
            [: Future constraints ]
            [: [codesnippet:where] or inline constraint syntax ]
            [: Deferred until traits are more settled. ]
        ]
    ]
]

#[section, $markdown:
    # Why generics are needed

    Traits and type unions are useful, but they do not replace generics.

    They solve different problems:

    - traits describe required behaviour
    - type unions describe a closed set of possible concrete types
    - generics describe a type relationship where concrete types are chosen by the caller and preserved through the API

    That last category is the important gap.

    Generics are needed for cases such as:

    - returning the same type that was passed in
    - expressing that two parameters must be the same concrete type
    - reusable typed containers such as [codesnippet:Box], [codesnippet:Option], [codesnippet:Result], [codesnippet:Array], [codesnippet:Map], or [codesnippet:Set]
    - reusable algorithms that preserve strong static typing
    - avoiding code duplication across many concrete types
    - avoiding weaker "accept anything" APIs that lose type precision

    Without generics, Beanstalk would eventually be pushed toward poor alternatives:

    - compiler-owned special cases for too many library patterns
    - duplicated implementations for many concrete types
    - weaker typing in reusable APIs
]

#[section, $markdown:
    # Design goals

    The generic system should follow the existing language style.

    Goals:

    - Keep syntax readable and keyword-based
    - Avoid punctuation-heavy or symbolic syntax
    - Preserve the one-concept-one-signal direction of the language
    - Keep generic declaration and generic application visually distinct
    - Support strong type inference where it is obvious and predictable
    - Keep implementation tractable in the frontend and type checker
    - Leave room for trait constraints later without forcing a redesign
    - Avoid introducing runtime generic machinery as a semantic requirement

    Generics should feel like authored Beanstalk syntax, not imported punctuation from another language.
]

#[section, $markdown:
    # Non-goals for the initial version

    The first implementation should not include:

    - higher-kinded types
    - const generics
    - specialization
    - lifetime parameters
    - generic associated types
    - advanced trait solving
    - complex implicit inference rules
    - dynamic generic reification as a language feature

    The initial version should solve ordinary generic functions, structs, and choices well.

    Anything that pushes Beanstalk toward Rust-level generic complexity should be deferred.
]

#[section, $markdown:
    # Core syntax decision

    The recommended syntax is:

    - [codesnippet:type] for declaring generic parameters
    - [codesnippet:of] for applying generic arguments

    This gives Beanstalk a clean and readable generic system without reusing symbols that already carry strong meaning elsewhere in the language.

    Example declaration:

    [codeblock, $code("bst"):
        identity type T |value T| -> T:
            return value
        ;
    ]

    Example type application:

    [codeblock, $code("bst"):
        value Box of Int = Box(42)
    ]
]

#[section, $markdown:
    # Generic function declarations

    Generic parameters are declared directly after the function name using [codesnippet:type].

    [codeblock, $code("bst"):
        identity type T |value T| -> T:
            return value
        ;
    ]

    Multiple generic parameters are comma-separated:

    [codeblock, $code("bst"):
        pair_first type A, B |left A, right B| -> A:
            return left
        ;
    ]

    In these examples:

    - [codesnippet:T], [codesnippet:A], and [codesnippet:B] are type parameters
    - the type parameters are scoped to the function declaration
    - the caller chooses the concrete types through use
    - the function body is checked against the generic relationship
]

#[section, $markdown:
    # Generic structs

    Structs can declare generic parameters after the struct name.

    [codeblock, $code("bst"):
        Box type T = |
            value T,
        |
    ]

    Multiple parameters follow the same pattern:

    [codeblock, $code("bst"):
        Pair type A, B = |
            left A,
            right B,
        |
    ]

    Generic structs are useful for containers and reusable data shapes where the stored type should remain precise.
]

#[section, $markdown:
    # Generic choices

    Choices can also be generic.

    [codeblock, $code("bst"):
        Option type T ::
            Some T,
            None,
        ;
    ]

    [codeblock, $code("bst"):
        Result type T, E ::
            Ok T,
            Err E,
        ;
    ]

    Generic choices are the natural basis for reusable option-like and result-like APIs.

    They let the language describe a stable shape while preserving the concrete payload types.
]

#[section, $markdown:
    # Generic type application

    Generic arguments are applied with [codesnippet:of].

    [codeblock, $code("bst"):
        value Box of Int = Box(42)
        result Result of String, Error = Result.Ok("hello")
        maybe_name Option of String = Option.Some("Nye")
    ]

    This gives Beanstalk a readable type-application form that scales cleanly:

    [codeblock, $code("bst"):
        Array of String
        Map of String, Int
        Result of Page, Error
        Signal of User
    ]

    [codesnippet:of] should be used in type positions.

    It should not imply runtime generic objects or dynamic type reification.
]

#[section, $markdown:
    # Function calls and inference

    Generic functions should rely on type inference by default.

    Common code should look like ordinary function calls:

    [codeblock, $code("bst"):
        identity type T |value T| -> T:
            return value
        ;

        number = identity(42)
        text = identity("hello")
    ]

    The compiler infers:

    - [codesnippet:T] is [codesnippet:Int] for [codesnippet:identity(42)]
    - [codesnippet:T] is [codesnippet:String] for [codesnippet, "identity(\"hello\")"]

    This keeps generic functions lightweight in normal code.
]

#[section, $markdown:
    # Explicit generic function application

    Explicit generic function application may be supported later if needed.

    Possible future syntax:

    [codeblock, $code("bst"):
        value = identity of Int(42)
    ]

    This does not need to be required in the first implementation.

    Recommended alpha approach:

    - support generic functions
    - infer generic arguments from value arguments where possible
    - support explicit [codesnippet:of] in type positions immediately
    - defer explicit generic call-site application unless implementation pressure makes it necessary

    This keeps the first implementation smaller and avoids settling parser edge cases too early.
]

#[section, $markdown:
    # Why this syntax fits Beanstalk

    [codesnippet:type] and [codesnippet:of] fit Beanstalk for several reasons.

    ## It is readable

    [codesnippet, "identity type T"] and [codesnippet, "Box of Int"] are easy to read.

    They look like Beanstalk syntax rather than imported generic punctuation.

    ## It preserves syntax ownership

    Beanstalk already assigns strong meaning to its punctuation and keywords.

    Keyword-based generics avoid overloading symbols that already have a stable role.

    ## It avoids angle bracket problems

    Classic angle-bracket generics are intentionally avoided.

    [codeblock, $code("bst"):
        Box<Int>
        identity<Int>(42)
    ]

    This style is rejected because it:

    - conflicts with Beanstalk's syntax philosophy
    - introduces punctuation-heavy parsing
    - creates nested readability issues
    - creates pressure around future type-grammar extensions
    - risks ambiguity in comparison-heavy contexts

    ## It scales across declaration kinds

    The same declaration pattern works for:

    - functions
    - structs
    - choices

    That consistency matters.
]

#[section, $markdown:
    # Future constraint syntax

    By using [codesnippet:type] only for parameter introduction and [codesnippet:of] only for application, future constraints can be added as a separate clause.

    Possible direction:

    [codeblock, $code("bst"):
        sort type T |items {T}| -> {T}
        where T is ORDERED:
            ...
        ;
    ]

    Another possible direction:

    [codeblock, $code("bst"):
        sort type T is ORDERED |items {T}| -> {T}:
            ...
        ;
    ]

    This should remain undecided until trait design is more settled.

    The important point is that generics should leave a clean place for trait constraints later.
]

#[section, $markdown:
    # Why traits are not enough

    Traits answer:

    > What behaviour must this type support?

    Example direction:

    [codeblock, $code("bst"):
        sort type T |items {T}| -> {T}
        where T is ORDERED:
            ...
        ;
    ]

    The trait constraint describes required behaviour.

    The generic parameter still matters because it preserves the actual concrete type through the API.

    Without [codesnippet:T], the function cannot express that the output collection contains the same concrete item type as the input collection.
]

#[section, $markdown:
    # Why type unions are not enough

    Type unions answer:

    > Which closed set of concrete types is accepted here?

    That is useful for intentionally closed APIs.

    But unions do not express:

    - the output is the same concrete type as the input
    - both parameters must be the same concrete type
    - a container stores some caller-chosen type [codesnippet:T]

    That is the core role of generics.

    Generics preserve type relationships.
]

#[section, $markdown:
    # Implementation principles

    The generic system should stay aligned with Beanstalk's broader compiler direction.

    Important principles:

    - Generics should be primarily a frontend type-system feature
    - Generics should not force a large semantic rewrite of borrowing or ownership
    - Static resolution should remain the default direction
    - Generics should not require runtime interface-object semantics
    - Generics should not become a large meta-programming system

    Beanstalk already emphasizes readability, strong static typing, compile speed, and backend flexibility.

    The generic system should reinforce those goals rather than complicate them.
]

#[section, $markdown:
    # Declaration rules

    Recommended initial declaration rules:

    - Generic parameters are introduced with [codesnippet:type]
    - Multiple parameters are comma-separated
    - Generic parameters are scoped to the declaration they belong to
    - Generic parameter names follow normal type-style naming conventions
    - Generic parameters may be used in parameter types, return types, fields, and choice payloads
    - Generic parameters should not leak outside their declaration scope

    Invalid use should produce direct diagnostics rather than falling through into unresolved type errors.
]

#[section, $markdown:
    # Use-site rules

    Recommended initial use-site rules:

    - Generic type application uses [codesnippet:of]
    - Multiple generic arguments are comma-separated
    - Generic type application is valid in normal type positions
    - Generic type application must provide the correct number of arguments
    - Generic arguments must resolve to types
    - Nested generic types should remain readable and unambiguous

    Example:

    [codeblock, $code("bst"):
        users {User} = {}
        pages Result of {Page}, Error = load_pages()!
        cache Map of String, User = create_cache()
    ]
]

#[section, $markdown:
    # Inference rules

    Generic function calls should infer arguments from normal value arguments where possible.

    Inference should remain predictable and conservative.

    If inference fails, the compiler should emit a clear diagnostic.

    Example:

    [codeblock, $code("bst"):
        identity type T |value T| -> T:
            return value
        ;

        value = identity(42)
    ]

    The compiler can infer [codesnippet:T] from the argument [codesnippet:42].

    If a type parameter cannot be inferred from arguments or context, the compiler should reject the call and explain which type parameter is unknown.
]

#[section, $markdown:
    # Diagnostics

    Diagnostics should be explicit and structured.

    Likely user-facing failures include:

    - wrong number of generic arguments
    - generic argument provided where none is expected
    - missing generic arguments in a type position where they are required
    - inference failure for a generic function call
    - use of a non-type symbol where a type argument is expected
    - use of constraint syntax before it is implemented
    - generic parameter name collision
    - generic parameter declared but never usable in the signature

    Generics touch type checking heavily.

    Bad generic code should produce specific messages, not vague type mismatch cascades.
]

#[section, $markdown:
    # Identity function example

    [codeblock, $code("bst"):
        identity type T |value T| -> T:
            return value
        ;

        x = identity(42)
        y = identity("hello")
    ]

    This preserves the input type.

    [codesnippet:x] is inferred as [codesnippet:Int].

    [codesnippet:y] is inferred as [codesnippet:String].
]

#[section, $markdown:
    # Generic struct example

    [codeblock, $code("bst"):
        Box type T = |
            value T,
        |

        number_box Box of Int = Box(42)
        text_box Box of String = Box([: hello ])
    ]

    [codesnippet, "Box of Int"] and [codesnippet, "Box of String"] share the same generic declaration but preserve different concrete value types.
]

#[section, $markdown:
    # Generic choice example

    [codeblock, $code("bst"):
        Result type T, E ::
            Ok T,
            Err E,
        ;

        parse_name || -> Result of String, Error:
            return Result.Ok("Nye")
        ;
    ]

    [codesnippet:Result] describes the shape.

    [codesnippet, "Result of String, Error"] describes a concrete use of that shape.
]

#[section, $markdown:
    # Relationship-preserving API example

    [codeblock, $code("bst"):
        first type T |items {T}| -> T:
            return items.get(0)!
        ;
    ]

    This is exactly the kind of API that traits or unions do not express cleanly by themselves.

    The function says:

    - the input is a collection of [codesnippet:T]
    - the return value is the same [codesnippet:T]
    - the caller chooses the concrete [codesnippet:T]

    That relationship is the point of generics.
]

#[section, $markdown:
    # Syntax alternatives considered

    ## Angle brackets

    [codeblock, $code("bst"):
        Box<Int>
        identity<Int>(42)
    ]

    Rejected.

    Reasons:

    - does not fit Beanstalk's syntax philosophy
    - visually noisier
    - creates future grammar pressure
    - weakens syntax ownership

    ## Symbol-based forms

    Symbol-heavy generic syntax is rejected for now.

    Reasons:

    - cryptic
    - visually dense
    - hard to scale cleanly
    - easy to regret later

    ## Type parameters as normal parameters

    Example direction:

    [codeblock, $code("bst"):
        identity |T Type, value T| -> T:
            return value
        ;
    ]

    Rejected for now.

    Reasons:

    - blurs value parameters and type parameters too much
    - makes generic parameters feel like runtime arguments
    - is less clean across structs and choices
    - introduces more magic into parameter parsing
]

#[section, $markdown:
    # Future extension points

    The syntax is designed to leave room for later features.

    Possible future additions:

    - trait constraints
    - generic methods where appropriate
    - explicit generic call-site application
    - standard library generic containers
    - compiler diagnostics tailored for inference and constraints

    Possible future constraint direction:

    [codeblock, $code("bst"):
        contains type T |items {T}, value T| -> Bool
        where T is EQUALS:
            ...
        ;
    ]

    Constraint syntax should be designed later, once the trait model is more stable.
]

#[section, $markdown:
    # Recommended alpha scope

    The initial implementation should stay narrow.

    Include:

    - generic functions
    - generic structs
    - generic choices
    - [codesnippet:type] declaration syntax
    - [codesnippet:of] type application syntax
    - simple predictable inference for function calls

    Defer:

    - advanced constraints
    - explicit generic call-site application unless necessary
    - advanced inference
    - generic methods if they complicate the implementation
    - any feature that pushes Beanstalk toward Rust-level generic complexity

    This is enough to unlock most useful generic APIs without turning generics into a compiler complexity trap.
]

#[section, $markdown:
    # Summary

    Beanstalk should adopt a minimal, keyword-based generic system built around:

    - [codesnippet:type] for generic parameter declaration
    - [codesnippet:of] for generic argument application

    This direction is:

    - readable
    - consistent
    - future-friendly
    - easy to teach
    - aligned with Beanstalk's syntax philosophy
    - restrained enough for an alpha-stage implementation

    The core rule is:

    > Beanstalk should support the useful 80% of generics without inheriting the full complexity of larger generic systems.
]