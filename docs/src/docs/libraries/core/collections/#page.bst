import @html {center}
import @styles/docs {navbar, title, section, codeblock, theme_head}

page_title #= "Core Collections"
page_description #= "Ordered collection literals and common collection methods."
page_head #= theme_head

#[navbar]
#[title, center: CORE COLLECTIONS]

#[section, $markdown:
    Collections are ordered, zero-indexed groups of one element type.
    Non-empty literals infer their element type from their items.
    Empty literals need an explicit collection type.
    Growable collections use `{T}`, while fixed collections use `{N T}`
    with a compile-time maximum length.

    [codeblock, $code("bst"):
        values ~= {1, 2, 3}
        empty_values ~{Int} = {}
        fixed_values ~{2 Int} = {}
    ]

    Common methods are compiler-owned and use receiver syntax:

    [codeblock, $code("bst"):
        ~values.push(4) catch:
        ;

        first = values.get(0) catch:
            then 0
        ;

        ~values.set(1, 99) catch:
        ;
    ]

    `get`, `set`, `push`, and `remove` are fallible.
    `length` is infallible.
    See @../../../collections (Collections) for the language rules around mutability, fixed capacity,
    and empty literal typing.
]
