import @html {center}
import @styles/docs {navbar, title, section, styled_table, header, data, codeblock, theme_head}

page_title #= "Traits in Beanstalk"
page_description #= "Current Alpha trait declarations, explicit conformance, and static generic bounds."
page_head #= theme_head

#[navbar]
#[title, center: TRAITS IN BEANSTALK]

#[section, $markdown:
    Traits define named behavioural contracts for types.

    Beanstalk traits are explicit and nominal: a type satisfies a trait only when source declares `Type must TRAIT`. Matching method shapes alone are not conformance.

    Trait names are static contracts. They can appear in trait declarations, explicit conformance declarations, and generic bounds. They are not ordinary value types.
]

#[section, $markdown:
    # Trait declarations

    Trait declarations use `must:` and contain method requirements.

    [codeblock, $code("bst"):
        DISPLAY_TEXT must:
            display |This| -> String
        ;
    ]

    Rules:

    - trait names are all caps
    - trait declarations are top-level declarations
    - requirements are method signatures only
    - empty marker traits are valid
    - static non-method requirements are deferred
    - default methods, associated types, associated constants, trait inheritance, generic traits, and generic trait methods are outside Beanstalk's trait design scope
]

#[section, $markdown:
    # This and mutable receivers

    `This` is valid only inside trait declarations. It means the concrete implementing type.

    [codeblock, $code("bst"):
        RESETTABLE must:
            reset |~This|
        ;
    ]

    Rules:

    - the first requirement parameter must be `This` or `~This`
    - `This` means an immutable receiver requirement
    - `~This` means a mutable receiver requirement
    - `~This` is valid only as the first parameter
    - direct non-receiver `This` parameters must be named, such as `other This`
    - bare non-receiver `This` is rejected because only the receiver may omit a name
    - direct `This` returns are supported
    - composed forms such as `This?`, `{This}`, and `Box of This` are rejected in the Alpha surface
    - lowercase `this` is not valid in trait requirements
    - `This` outside trait declarations is rejected
]

#[section, $markdown:
    # Explicit conformance

    A conformance declaration states that a type implements one or more traits.

    [codeblock, $code("bst"):
        Label = |
            text String,
        |

        display |this Label| -> String:
            return this.text
        ;

        Label must DISPLAY_TEXT
    ]

    Conformance declarations are bodyless, top-level declarations. They do not end with a semicolon.

    Multiple traits can be listed with commas:

    [codeblock, $code("bst"):
        Label must DISPLAY_TEXT, SERIALIZABLE
    ]

    The compiler validates that every required method exists with the exact receiver mutability, parameter types, return types, and return channels required by the trait. Parameter names are not part of the contract.
]

#[section, $markdown:
    # Canonical evidence

    Beanstalk stores explicit conformance as evidence selected by the frontend.

    [styled_table:
        [header: [: Evidence] [: When it is created] [: Visibility] ]

        [data:
            [: Canonical]
            [: The conformance target is a struct, choice, or generic type constructor declared in the same file.]
            [: Reusable wherever both the target type and trait are visible.]
        ]
    ]

    User-authored conformances for builtin, imported, dependency/library, external opaque, and
    other-file types are rejected. Use a local wrapper choice or struct when adapting values owned
    by another file or package.
]

#[section, $markdown:
    # Static generic bounds

    Generic bounds use `is`, not `must`.

    [codeblock, $code("bst"):
        render type Item is DISPLAY_TEXT |item Item| -> String:
            return item.display()
        ;
    ]

    Multiple bounds on one parameter use `and`:

    [codeblock, $code("bst"):
        render type Item is DISPLAY_TEXT and NAMED |item Item| -> String:
            return item.display()
        ;
    ]

    Concrete calls and generic struct/choice instantiations require visible reusable evidence for the chosen concrete type. Trait names cannot be supplied as value types or concrete generic arguments.
]

#[section, $markdown:
    # Traits are not value types

    A trait name in a normal type annotation is rejected.

    [codeblock, $code("bst"):
        -- invalid: DISPLAY_TEXT is a trait, not a value type
        render |value DISPLAY_TEXT| -> String:
            return value.display()
        ;
    ]

    Use `type T is TRAIT` when static reuse is intended. Use choices when runtime heterogeneity is needed.
]

#[section, $markdown:
    # Runtime heterogeneity

    Runtime heterogeneity is represented with choices, not trait objects.

    [codeblock, $code("bst"):
        Renderable ::
            LabelItem | value Label |,
            ButtonItem | value Button |,
        ;
    ]
]

#[section, $markdown:
    # Dispatch and backends

    Trait bounds may enable receiver calls on generic parameters inside the bounded generic declaration. Concrete values use ordinary visible receiver methods. A conformance declaration proves that a type satisfies a trait; it does not independently import trait methods as concrete receiver methods.

    Static trait-bound method calls resolve to concrete source methods before HIR. Backends do not receive trait-object construction, erased dispatch, or selected trait evidence for runtime method tables.
]

#[section, $markdown:
    # DISPLAYABLE scaffold

    `DISPLAYABLE` exists as compiler-owned trait metadata for conformance and generic-bound tests.

    It does not change `io(...)`, template interpolation, string coercion, equality, or operators. Implement `display |This| -> String` yourself and call it explicitly.
]

#[section, $markdown:
    # Outside trait design scope

    These surfaces are intentionally outside Beanstalk's trait design scope:

    - default methods
    - associated types and associated constants
    - trait inheritance and composition
    - generic traits and generic trait methods
    - blanket, conditional, negative, or specialized conformance
    - structural conformance
    - specialized generic instance conformance such as `Box of Int must DISPLAY_TEXT`
    - composed `This` forms
    - automatic primitive conformances
    - output coercion through `DISPLAYABLE`
    - operator-to-trait integration
    - deriving, hashing, ordering, formatting, iteration, and serialization traits
    - dynamic trait values / trait objects
    - trait aliases or composition, downcasting, reflection, and pattern matching on erased concrete types
]

#[section, $markdown:
    # Deferred trait work

    Deferred trait work is limited to static non-method requirements, compiler-owned builtin
    conformance facts, diagnostics/tooling polish, and broader standard trait taxonomy that does
    not change traits into runtime values or a type-level programming system.
]
