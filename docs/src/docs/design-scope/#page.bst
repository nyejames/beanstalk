import @html {center}
import @styles/docs {navbar, title, section, styled_table, header, data, codeblock, theme_head}

page_title #= "Language Design Scope"
page_description #= "What belongs in Beanstalk's language design, and what is intentionally outside it."
page_head #= theme_head

#[navbar]
#[title, center: LANGUAGE DESIGN SCOPE]

#[section, $markdown:
    Beanstalk keeps the source language deliberately small. Compiler complexity is acceptable when
    it makes the programmer-facing model simpler, safer, and easier to read.

    Future work is separated into two categories:

    - **Deferred**: fits Beanstalk's design, but is not implemented yet or is not part of Alpha.
    - **Outside language design scope**: intentionally not planned because it would make the
      language broader, more implicit, more solver-heavy, or harder to reason about.
]

#[section, $markdown:
    # Static reuse

    Use generic trait bounds when code should be reused statically across types.

    [codeblock, $code("bst"):
        render type Item is DISPLAY_TEXT |item Item| -> String:
            return item.display()
        ;
    ]
]

#[section, $markdown:
    # Runtime heterogeneity

    Use choices when a value should carry one of several concrete runtime shapes.

    [codeblock, $code("bst"):
        Renderable ::
            LabelItem | value Label |,
            ButtonItem | value Button |,
        ;
    ]
]

#[section, $markdown:
    # Outside design scope

    [styled_table:
        [header: [: Surface] [: Reason] ]

        [data:
            [: Dynamic trait values and trait objects]
            [: Traits are static contracts. Runtime heterogeneity should use choices.]
        ]

        [data:
            [: Broad trait ecosystems]
            [: Default methods, associated items, inheritance, composition, generic traits, and specialized conformance create a type-level programming system.]
        ]

        [data:
            [: General macro systems]
            [: Templates, constants, and builder directives are the constrained compile-time surface.]
        ]

        [data:
            [: User-defined builtin-map keys]
            [: Builtin hashmaps stay scalar-keyed. More specialized maps should be library or user-defined structs.]
        ]

        [data:
            [: Type-level programming]
            [: Type values, reflection, higher-kinded types, partial application, and parameterized aliases make code harder to inspect.]
        ]
    ]
]
