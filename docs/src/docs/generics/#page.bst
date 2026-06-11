import @html {center}
import @styles/docs {navbar, title, section, styled_table, header, data, codeblock, theme_head}

page_title #= "Generics in Beanstalk"
page_description #= "Current Alpha generic type and generic function behavior."
page_head #= theme_head

#[navbar]
#[title, center: GENERICS IN BEANSTALK]

#[section, $markdown:
    Generics are partially implemented for the current Alpha compiler.

    The supported surface is intentionally small:

    - declaration-site generic parameters with `type` on top-level structs, choices, and free functions
    - `of` type application in type positions
    - concrete generic aliases to fully applied generic types
    - constructor inference for concrete generic structs and choices
    - collections as compiler-owned generic constructed types using `{T}`
    - generic free-function calls across same-file use, visible same-module imports, and authored facades
    - declaration-site trait bounds with `type T is TRAIT`

    Generic function headers are parsed and tracked by the frontend, and their bodies are
    validated against unconstrained generic parameters before any concrete call is accepted.
    Same-file calls, grouped cross-file imports, namespace cross-file imports, and authored facade wrapper calls infer
    type arguments from immediate arguments and immediate expected result context. Facade wrappers
    resolve their bodies through the facade file's own imports, so private implementation helpers
    stay hidden behind real public wrapper declarations.
]

#[section, $markdown:
    # Current status

    [styled_table:
        [header: [: Surface] [: Status] [: Notes] ]

        [data:
            [: Generic structs]
            [: Supported]
            [: Concrete instances such as [$html:<code>Box of Int</code>] have canonical frontend type identity and substituted field views.]
        ]

        [data:
            [: Generic choices]
            [: Supported]
            [: Concrete constructors and matching work for instantiated generic choice types.]
        ]

        [data:
            [: Concrete generic aliases]
            [: Supported]
            [: Aliases may target fully applied generic instances and remain transparent.]
        ]

        [data:
            [: Generic functions]
            [: Partial]
            [: Same-file, same-module grouped or namespace, and authored facade wrapper free-function calls infer from immediate arguments and type context, then emit concrete executable instances. Generic optional and fallible returns work across visible imports.]
        ]

        [data:
            [: Generic aliases with parameters]
            [: Deferred]
            [: Alias only a fully concrete generic instance for now.]
        ]

        [data:
            [: Explicit generic call syntax]
            [: Rejected / Not Alpha]
            [: Generic calls use ordinary call syntax and inference only.]
        ]

        [data:
            [: Generic function values]
            [: Rejected / Deferred]
            [: Generic functions are callable declarations only, not first-class values.]
        ]

        [data:
            [: Generic receiver methods]
            [: Partial]
            [: Declaration-site generic receiver methods are supported when the method generic parameters align exactly with the receiver constructor arguments. Bound-provided receiver calls are available through visible reusable trait evidence; receiver methods on fully instantiated generic types remain rejected.]
        ]

        [data:
            [: Type values / type-level CTFE]
            [: Not supported]
            [: Types are compile-time annotation facts, not runtime or compile-time values.]
        ]

        [data:
            [: Trait bounds]
            [: Partial]
            [: Declaration-site [$html:<code>type T is TRAIT</code>] and [$html:<code>type T is A and B, U is C</code>] bounds are supported for generic functions, structs, and choices. [$html:<code>where</code>] clauses and file-local evidence-backed generic bound dispatch remain deferred.]
        ]
    ]
]

#[section, $markdown:
    # Generic type syntax

    Generic parameters are introduced with `type`.

    [codeblock, $code("bst"):
        Box type T = |
            value T,
        |
    ]

    Concrete type arguments are applied with `of` in type positions.

    [codeblock, $code("bst"):
        number_box Box of Int = Box(42)
        text_box Box of String = Box("hello")
    ]

    `of` is type-position-only. It is not current Alpha syntax for explicit
    generic function calls, runtime generic values, type values, or type-level computation.
]

#[section, $markdown:
    # Generic choices

    Generic choices use the same `type` parameter list.

    Payload variants use record payload syntax.

    [codeblock, $code("bst"):
        Maybe type T ::
            Some | value T |,
            Empty,
        ;

        name Maybe of String = Maybe::Some("Ana")
        missing Maybe of String = Maybe::Empty
    ]

    Choice payload shorthand such as `Some T` is not current Alpha syntax.
    Use record payload variants such as `Some | value T |`.
]

#[section, $markdown:
    # Concrete aliases

    Type aliases can name a fully applied generic instance.

    [codeblock, $code("bst"):
        Box type T = |
            value T,
        |

        StringBox as Box of String

        item StringBox = Box("docs")
        raw Box of String = item
    ]

    Generic aliases with their own parameters, partial application, and type-constructor
    composition are deferred. Name an intermediate concrete alias when a type would otherwise
    need deeper composition.
]

#[section, $markdown:
    # Collections and constructed types

    Collections use the public `{T}` syntax and are compiler-owned constructed
    types. Fixed collections use `{N T}`, and the fixed capacity is part of type
    identity rather than allocation metadata.

    [codeblock, $code("bst"):
        names {String} = {"Ana", "Nye"}

        cache_capacity #Int = 64
        cached ~{cache_capacity Box of String} = {}
    ]

    Optional types `T?` and fallible return slots such as `Error!`
    are also compiler-owned constructed forms. Public `Option of T`,
    public `Result of T, E`, `Option.Some`, and
    `Result.Ok` are not current Alpha syntax.

    Generic functions can use final generic error slots, `return!`,
    postfix propagation, and `catch` when ordinary local inference can solve the
    generic parameters. The compiler still rejects body-side specialization of an abstract
    generic error slot.
]

#[section, $markdown:
    # Nesting rule

    Alpha allows one inline `of` application per annotation.

    Collection element annotations may contain that one application:

    [codeblock, $code("bst"):
        boxes {Box of String} = {}
    ]

    Nested inline applications are deferred:

    [codeblock, $code("bst"):
        -- deferred
        value Box of Pair of String, Int = Box(Pair("count", 3))
    ]

    Use a concrete alias instead:

    [codeblock, $code("bst"):
        StringCount as Pair of String, Int
        value Box of StringCount = Box(Pair("count", 3))
    ]
]

#[section, $markdown:
    # Generic functions

    Generic free functions are callable when their type arguments can be inferred from the call
    itself or from an immediate expected result type. The same inference rules apply for same-file
    calls, grouped imports, namespace imports, and authored facade wrappers. There is no explicit
    generic call-site type argument syntax.

    [codeblock, $code("bst"):
        identity type T |value T| -> T:
            return value
        ;

        value Int = identity(42)
        text String = identity("hello")
    ]

    Return type context can also fix otherwise ambiguous type parameters:

    [codeblock, $code("bst"):
        empty_values type T || -> {T}:
            return {}
        ;

        numbers {Int} = empty_values()
    ]

    Immediate expected-result inference also works across visible imports:

    [codeblock, $code("bst"):
        -- helpers.bst
        empty_values type T || -> {T}:
            return {}
        ;

        -- #page.bst
        import @./helpers {empty_values}

        numbers {Int} = empty_values()
    ]

    Body validation currently allows pass-through behavior such as returning a value of the
    same generic parameter type, forwarding it to another generic function, or storing it in
    a generic wrapper. Local annotations inside the generic body may use the active generic
    parameters, including `T`, `{T}`, and `Box of T`.
    Behavior-dependent operations on unconstrained `T` are rejected. A declaration-site
    trait bound can allow receiver calls supplied by a unique visible trait requirement. Arithmetic,
    equality, field access, template interpolation, and external/IO behavior still require concrete
    support or a future dedicated trait integration.

    Inference is deliberately immediate. Beanstalk does not infer a generic function's type
    arguments from later uses of the resulting value, whole-program analysis, HIR, borrow
    validation, or expected parameter context from an outer function call into a nested generic
    call:

    [codeblock, $code("bst"):
        -- rejected: T is ambiguous at the declaration site
        values = empty_values()
    ]

    Use an intermediate annotation when a nested generic call would otherwise need evidence from
    its parent call:

    [codeblock, $code("bst"):
        user User = parse_json(text)!
        save_user(user)
    ]

    Generic functions may also return optional or fallible values, including across imports:

    [codeblock, $code("bst"):
        -- helpers.bst
        wrap_optional type T |value T| -> T?:
            return value
        ;

        read_or_raise type T, E |value T, err E, fail Bool| -> T, E!:
            if fail:
                return! err
            ;

            return value
        ;

        -- #page.bst
        import @./helpers {read_or_raise, wrap_optional}

        maybe_name String? = wrap_optional("Ana")

        value = read_or_raise(7, Error("bad"), false) catch:
            then 0
        ;
    ]

    Generic optional and fallible support still depends on immediate evidence. A bare
    `none` cannot infer `T` by itself, and a concrete callee error type
    cannot specialize an abstract surrounding `E!` from inside the generic body.

    Same-module grouped imports can call generic free functions by their imported local name:

    [codeblock, $code("bst"):
        -- helpers.bst
        identity type T |value T| -> T:
            return value
        ;

        -- #page.bst
        import @./helpers {identity}

        value = identity(42)
    ]

    Namespace imports can call generic free functions through the shallow namespace record:

    [codeblock, $code("bst"):
        -- #page.bst
        import @./helpers as helpers

        value = helpers.identity(42)
    ]

    Public facade wrappers are ordinary authored generic functions marked with `export` in `#mod.bst`.
    Normal imports inside the facade remain private and are only usable by the wrapper body:

    [codeblock, $code("bst"):
        -- lib/utils/internal.bst
        identity_impl type T |value T| -> T:
            return value
        ;

        -- lib/utils/#mod.bst
        import @./internal {identity_impl}

        export identity type T |value T| -> T:
            return identity_impl(value)
        ;

        -- src/#page.bst
        import @utils {identity}

        value = identity(42)
    ]

    Namespace imports can call authored facade wrappers too:

    [codeblock, $code("bst"):
        -- src/#page.bst
        import @utils as utils

        value = utils.identity("hello")
    ]

    Generic functions are still callable declarations only. They cannot be passed around as
    first-class values, including through namespace members such as `helpers.identity`.
    Direct facade re-export syntax is also deferred; expose imported behavior with real authored
    wrapper declarations.
]

#[section, $markdown:
    # Trait bounds

    Generic bounds use `is`, not `must`.

    [codeblock, $code("bst"):
        render type Item is DISPLAY_TEXT |item Item| -> String:
            return item.display()
        ;
    ]

    Multiple bounds on one parameter use `and`. Commas still separate generic parameters:

    [codeblock, $code("bst"):
        combine type A is DISPLAY_TEXT, B is DISPLAY_TEXT |left A, right B| -> String:
            return left.display() + right.display()
        ;

        render_named type Item is DISPLAY_TEXT and NAMED |item Item| -> String:
            return item.display()
        ;
    ]

    Concrete generic function calls and concrete generic struct/choice instantiations require
    explicit visible reusable evidence for each concrete type argument. File-local extension
    evidence can support local concrete fallback, but it cannot satisfy public/static generic API
    obligations across files.

    [codeblock, $code("bst"):
        Label = |
            text String,
        |

        display |this Label| -> String:
            return this.text
        ;

        Label must DISPLAY_TEXT

        value = render(Label("ok"))
    ]

    Trait names are static contracts. They can appear in generic bounds, trait declarations, and
    conformance declarations, but not as normal value types or concrete generic arguments.

    Bound-provided receiver methods use the selected static evidence and lower to concrete calls
    before HIR.
    `where` clauses remain rejected; write bounds directly on the generic parameter list.
]

#[section, $markdown:
    # Deferred features

    The following are not current Alpha generics:

    - explicit generic call syntax such as `identity of Int(42)`, `identity<Int>(42)`, `["identity[Int](42)"]`, or `identity(42 Int)`
    - inline generic sugar such as `|value type A|`
    - generic function values and higher-order generic functions
    - `where` clauses and file-local evidence-backed generic bound dispatch
    - type values, type-returning functions, type-level `#if`, and compile-time type inspection
    - recursive generic structs or choices
    - receiver methods on instantiated generic types
    - namespace or wildcard facade exports and function alias exports through `#mod.bst`
    - generic JS or external package functions
    - generic aliases with parameters
    - partial type application
    - nested generic type application beyond the collection element exception
    - public `Option of T` or `Result of T, E`
    - `Option.Some`, `Result.Ok`, public `Ok` or `Err` constructors
    - higher-kinded types, const generics, lifetime parameters, specialization, associated types, and dynamic generic reification
]
