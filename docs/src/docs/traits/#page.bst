import @html {center}
import @styles/docs {navbar, title, section, styled_table, header, data, codeblock, codesnippet, theme_head}

page_title #= "Traits in Beanstalk"
page_description #= "Current Alpha trait declarations, explicit conformance, static bounds, and dynamic trait values."
page_head #= theme_head

#[navbar]
#[title, center: TRAITS IN BEANSTALK]

#[section, $markdown:
    Traits define named behavioural contracts for types.

    Beanstalk traits are explicit and nominal: a type satisfies a trait only when source declares [codesnippet:Type must TRAIT]. Matching method shapes alone are not conformance.

    A trait name in a normal type annotation means a dynamic trait value.
    A trait name in a generic bound constrains a concrete generic parameter.
    These are different features.
]

#[section, $markdown:
    # Trait declarations

    Trait declarations use [codesnippet:must:] and contain method requirements.

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
    - default methods, associated types, associated constants, trait inheritance, generic traits, and generic trait methods are deferred
]

#[section, $markdown:
    # This and mutable receivers

    [codesnippet:This] is valid only inside trait declarations. It means the concrete implementing type.

    [codeblock, $code("bst"):
        RESETTABLE must:
            reset |~This|
        ;
    ]

    Rules:

    - the first requirement parameter must be [codesnippet:This] or [codesnippet:~This]
    - [codesnippet:This] means an immutable receiver requirement
    - [codesnippet:~This] means a mutable receiver requirement
    - [codesnippet:~This] is valid only as the first parameter
    - lowercase [codesnippet:this] is not valid in trait requirements
    - [codesnippet:This] outside trait declarations is rejected
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
    # Canonical and file-local evidence

    Beanstalk stores explicit conformance as evidence selected by the frontend.

    [styled_table:
        [header: [: Evidence] [: When it is created] [: Visibility] ]

        [data:
            [: Canonical]
            [: The conformance target is a struct, choice, or generic type constructor declared in the same file.]
            [: Reusable wherever both the target type and trait are visible.]
        ]

        [data:
            [: File-local extension]
            [: The conformance target is imported, builtin, external, or otherwise not declared in the current file.]
            [: Usable only in the declaring file.]
        ]
    ]

    File-local extension evidence is useful for local adapter code, but it is never exportable as a reusable conformance fact. It also cannot override visible canonical evidence.
]

#[section, $markdown:
    # Static generic bounds

    Generic bounds use [codesnippet:is], not [codesnippet:must].

    [codeblock, $code("bst"):
        render type Item is DISPLAY_TEXT |item Item| -> String:
            return item.display()
        ;
    ]

    Multiple bounds on one parameter use [codesnippet:and]:

    [codeblock, $code("bst"):
        render type Item is DISPLAY_TEXT and NAMED |item Item| -> String:
            return item.display()
        ;
    ]

    Concrete calls and generic struct/choice instantiations require visible reusable evidence for the chosen concrete type. A dynamic trait value does not satisfy a static generic bound.
]

#[section, $markdown:
    # Dynamic trait values

    A trait name in a normal type annotation creates a dynamic trait value type.

    [codeblock, $code("bst"):
        render_dynamic |value DISPLAY_TEXT| -> String:
            return value.display()
        ;

        text = render_dynamic(Label("hello"))
    ]

    Dynamic trait values erase the concrete type and expose only the trait surface. They are opaque owning wrappers in the current Alpha surface.

    Dynamic coercion from a concrete value happens only at explicit typed boundaries:

    - annotated declarations
    - function arguments
    - function returns
    - struct fields
    - choice payloads
    - explicitly typed collection elements

    Unannotated locals and collection literals do not infer trait values.
]

#[section, $markdown:
    # Dynamic-safe traits

    All traits can be used as generic bounds. Only dynamic-safe traits can be used as dynamic value types.

    A trait is dynamic-safe when its requirements do not need the erased concrete type again after wrapping. Requirements that return [codesnippet:This], take [codesnippet:This] as a non-receiver parameter, or otherwise require concrete identity are bound-only and cannot be used as dynamic value annotations.

    When static dispatch is intended, write a generic bound with [codesnippet:type T is TRAIT] instead of annotating a value directly with [codesnippet:TRAIT].
]

#[section, $markdown:
    # Dispatch and backends

    Ordinary concrete receiver methods resolve before trait evidence. If no ordinary method is visible, visible trait evidence can make a trait-surface method callable.

    Dynamic trait method calls dispatch through the wrapper's method table in the JavaScript backend. HTML-Wasm rejects reachable dynamic trait construction and dynamic dispatch with a structured unsupported-backend diagnostic before Wasm lowering. Dynamic-only helper functions that are not reachable from the page entry do not block an HTML-Wasm build.
]

#[section, $markdown:
    # DISPLAYABLE scaffold

    [codesnippet:DISPLAYABLE] exists as compiler-owned trait metadata for conformance and generic-bound tests.

    It does not change [codesnippet:io(...)], template interpolation, string coercion, equality, or operators. Implement [codesnippet:display |This| -> String] yourself and call it explicitly.
]

#[section, $markdown:
    # Deferred trait ecosystem

    Deferred surfaces include:

    - default methods
    - associated types and associated constants
    - trait inheritance and composition
    - generic traits and generic trait methods
    - specialized generic instance conformance such as [codesnippet:Box of Int must DISPLAY_TEXT]
    - dynamic trait aliases
    - downcasting, reflection, and pattern matching on erased concrete types
    - file-local evidence-backed generic bound dispatch
    - automatic primitive conformances
    - output coercion through [codesnippet:DISPLAYABLE]
    - operator-to-trait integration
    - deriving, hashing, ordering, formatting, iteration, and serialization traits
]
