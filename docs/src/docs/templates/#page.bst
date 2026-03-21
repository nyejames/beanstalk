import @lib/html/{format, center}
import @styles/docs/{navbar, footer, title, section, header, body, palette, table, row, header_row, codeblock}

#[header]
#[body:
[navbar]
[title, center: TEMPLATES]

[section, $markdown:
    Beanstalk has unique, superpowered templating strings. Templates use [$code:[]] exclusively - never confuse with collections [$code:{}].

    These are the only way to create mutable strings, as double quotes are only used for string slices.

    The main purpose of a template is to generate either complex text content, markdown, HTML or just regular formatted strings.

    Templates have two sections, the head and the body, separated with a colon. 

    Templates are scoped using square brackets and can be passed special types called Styles to change their behavior, content and styling.
    
    Templates are either folded to strings at compile time, or become functions that return strings at runtime. 

    ## Structure
    - Head and body separated by `:`
    - Variable capture from the surrounding scope

    Templates unlock the full power of Beanstalk's HTML / CSS generation capabilities.

    ## Styles
    Templates can be used to build complex UI components. They can use slots to insert content from other templates and have **style metadata** attached to them.

    A template’s style is defined in the **template head** using `$` directives. 
    `$` introduces **compiler-handled directives** (so they don’t collide with normal variables and can be extended in the future), such as formatter-like built-ins, precedence controls, and default child templates that are automatically applied to direct child templates.

    Directive availability is frontend-registry based:
    - Compiler built-ins are available by default (`$markdown`, `$code`, `$css`, slots, etc.).
    - Project builders can register additional directives using the same `$name` syntax.
    - Unknown directives fail as syntax/rule errors unless they are registered.

    [codeblock, $code("bst"):
        -- Define a template style
        [$markdown, $children([: All children start with this prefix! ]):
            # Hello
            This template is parsed as markdown.

            @example.com (Here is a link!) using this custom markdown flavour.

            [$todo: write some more info!]

            [: This child is prefixed!]
        ]
    ]

    **Built-in Style Directives**

    - $slot / $insert(..) - See slots below!
    - $reset              - Clears inherited style state before applying new directives in the same head
    - $markdown           - Parses the template bodies with a custom flavour of Markdown
    - $raw                - Parses the template body as a raw string (no escaping, no formatting)
    - $html               - Parses the template body as raw HTML (no escaping)
    - $css                - Provides some basic warnings for malformed CSS
    - $note / $todo       - Comments (ignored by final output)
    - $doc                - Turns the template into a documentation comment
    - $children(..)       - Accepts a template (or string slice) that will be applied only to this template's direct child templates

    Formatting directives do not automatically flow into nested child templates.
    If a child template should keep using a formatter such as `$markdown`, redeclare it in that child template's head.

    ## Slots

    Template slots let one template receive content from another template. The default slot is written as `[$slot]` and marks where the main body content should be inserted.

    Named slots can also be declared with `[$slot("name")]`. These allow helper templates to insert content into a specific part of another template using `$insert("name")`.

    Positional slots can be declared using positive integers, such as `[$slot(1)]`, `[$slot(2)]`, etc. Loose contributions (those not explicitly targetting a named slot) are assigned to positional slots first, in ascending numeric order. Any remaining loose contributions go to the default slot if it exists.

    [codeblock, $code("bst"):
        img = [:
            <img src="[$slot(1)]" alt="[$slot]">
        ]

        [img, "logo.png": Site logo]
    ]

    In this example, `"logo.png"` fills the first positional slot `[$slot(1)]`, and `"Site logo"` fills the default slot `[$slot]`.

    Named inserts:
    [codeblock, $code("bst"):
        title = [:
            <h1 style="[$slot("style")]">
                [$slot]
            </h1>
        ]

        blue = [$insert("style"): color: blue;]

        [title, blue:
            Hello world
        ]
    ]

    In this example, `blue` inserts `color: blue;` into the `style` slot of `title`, while `Hello world` is inserted into the default slot.

    If a template has named or positional slots but no default slot, any loose body content that cannot be assigned to a positional slot is an error. 
    If a slot receives no content, it expands to an empty string.
    Repeated slots, such as two occurrences of `[$slot(1)]`, will replay the same content in both places.

    Because `$children(..)` only applies to direct children, nested helpers can scope row and cell wrappers independently:

    [codeblock, $code("bst"):
        # table = [$children([:<tr>[$slot]</tr>]):
            <table style="[$slot("style")]">
                [$children([:<td>[$slot]</td>]):[$slot]]
            </table>
        ]

        [table:
            [: [: Type] [: Description] ]
            [: [: float ] [: 64 bit floating point number] ]
        ]
    ]

    In that example, each direct child of `table` becomes a row, while each direct child of a row becomes a cell. The outer `<tr>` wrapper does not leak into the inner cell templates.

    ## Template control flow examples
    [codeblock, $code("bst"):
        [head_of_element: content of element that renders as text (markdown / metadata / alt tag / code ..etc)]

        [ loop text in text_array: 
            [text]
        ]

        [if not text.isBlank():
            [ text: plus some extra text in the same element as the variable text ]
        ]
    ]

    ## Errors
    If an error is added to a template at runtime, it will be displayed in the console, rather than injected into the template. 
]

]-- end of body
[footer]