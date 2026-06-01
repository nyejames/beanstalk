import @html {center}
import @styles/docs {navbar, title, section, codeblock, codesnippet, theme_head}

page_title #= "Core Collections"
page_description #= "Ordered collection literals and common collection methods."
page_head #= theme_head

#[navbar]
#[title, center: CORE COLLECTIONS]

#[section, $markdown:
    Collections are ordered, zero-indexed groups of one element type.
    Non-empty literals infer their element type from their items.
    Empty literals need an explicit collection type.

    [codeblock, $code("bst"):
        values ~= {1, 2, 3}
        empty_values ~{Int} = {}
    ]

    Common methods are compiler-owned and use receiver syntax:

    [codeblock, $code("bst"):
        ~values.push(4)

        first = values.get(0) catch:
            then 0
        ;

        ~values.set(1, 99) catch:
        ;
    ]

    [codesnippet:get], [codesnippet:set], and [codesnippet:remove] are fallible.
    [codesnippet:push] and [codesnippet:length] are infallible.
    See @../../../collections (Collections) for the language rules around mutability and empty literal typing.
]
