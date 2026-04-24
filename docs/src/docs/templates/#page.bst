import @lib/html {format, style}
import @styles/docs {navbar, title, section, palette, table, header, codeblock, codesnippet, theme_head}

# page_title = "Templates"
# page_description = "Template syntax, slots, directives, and formatting in Beanstalk."
# page_head = theme_head

#[navbar]
#[title, style.center: TEMPLATES]

#[section, $markdown:
    Beanstalk has unique, superpowered templating strings. Templates use [$code:[]] exclusively - never confuse with collections [$code:{}].

    These are the only way to create mutable strings, as double quotes are only used for string slices.

    The main purpose of a template is to generate either complex text content, markdown, HTML or just regular formatted strings.

    Templates have two sections separated by a colon:
    - Head
    - Body

    This example creates the string [codesnippet: "head body"]
    [codeblock, $code("bst"):
        ["head": body]
    ]

    Templates are scoped using square brackets and can be passed special types called Styles to change their behavior, content and styling.
    
    Templates are either folded to strings at compile time, or become functions that return strings at runtime. 

    ## Structure
    - Head and body separated by [codesnippet::]
    - Variable capture from the surrounding scope

    Templates unlock the full power of Beanstalk's HTML / CSS generation capabilities.

    ## Styles
    Templates can be used to build complex UI components. They can use slots to insert content from other templates and have **style metadata** attached to them.

    A template's style is defined in the **template head** using [codesnippet: $] directives. 
    [codesnippet:$] introduces **compiler-handled directives** (so they don't collide with normal variables and can be extended in the future), such as formatter-like built-ins, precedence controls, and default child templates that are automatically applied to direct child templates.

    Directive availability is frontend-registry based:
    - Frontend built-ins are available by default ([codesnippet:$markdown], [codesnippet:$code], [codesnippet:$raw], [codesnippet:$slot], etc.).
    - Project builders can register additional project-specific directives using the same [codesnippet:$name] syntax. In the HTML project, that includes [codesnippet:$html], [codesnippet:$css], and [codesnippet:$escape_html].
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

    **Frontend Built-in Style Directives**

    - [codesnippet:$slot] / [codesnippet:$insert(..)] - See slots below!
    - [codesnippet:$fresh]              - Opts this child template out of wrappers applied by the immediate parent's [codesnippet:$children(..)] directive
    - [codesnippet:$markdown]           - Parses the template bodies with a custom flavour of Markdown
    - [codesnippet:$code]               - Highlights code blocks using the compiler's built-in formatter
    - [codesnippet:$raw]                - Parses the template body as a raw string (no escaping, no formatting)
    - [codesnippet:$note / $todo]       - Comments (ignored by final output)
    - [codesnippet:$doc]                - Turns the template into a documentation comment
    - [codesnippet:$children(..)]       - Accepts a template (or string slice) that will be applied only to this template's direct child templates

    **HTML Project Directives**

    - [codesnippet:$html]               - Parses the template body as raw HTML (no escaping)
    - [codesnippet:$css]                - Provides some basic warnings for malformed CSS
    - [codesnippet:$escape_html]        - Escapes HTML-sensitive characters in the template body

    Formatting directives do not automatically flow into nested child templates.
    If a child template should keep using a formatter such as [codesnippet:$markdown], redeclare it in that child template's head.

    [codesnippet:$fresh] is per-child and only affects wrapper application from the immediate parent. Siblings without [codesnippet:$fresh] still receive the parent wrappers:

    [codeblock, $code("bst"):
        # list = [$children([:<li>[$slot]</li>]):
            <ul>
                [$slot]
            </ul>
        ]

        [list:
            [: one ]
            [$fresh: [: two ]]
        ]
    ]

    In this example, `one` is wrapped with `<li>...</li>`, while `two` opts out and is rendered without the parent [codesnippet:$children(..)] wrapper.

    ## Slots

    Template slots let one template receive content from another template. The default slot is written as [codesnippet:[$slot]] and marks where the main body content should be inserted.

    Named slots can also be declared with [codesnippet:[$slot("name")]]. These allow helper templates to insert content into a specific part of another template using [codesnippet:$insert("name")].

    Positional slots can be declared using positive integers, such as [codesnippet:[$slot(1)]], [codesnippet:[$slot(2)]], etc. Loose contributions (those not explicitly targetting a named slot) are assigned to positional slots first, in ascending numeric order. Any remaining loose contributions go to the default slot if it exists.

    [codeblock, $code("bst"):
        img = [:
            <img src="[$slot(1)]" alt="[$slot]">
        ]

        [img, "logo.png": Site logo]
    ]

    In this example, [codesnippet:"logo.png"] fills the first positional slot [codesnippet:[$slot(1)]], and [codesnippet:"Site logo"] fills the default [codesnippet:$slot].

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

    In this example, `blue` inserts [codesnippet:color: blue;] into the [codesnippet:$slot("style")] slot of `title`, while `Hello world` is inserted into the default [codesnippet:$slot].

    If a template has named or positional slots but no default slot, any loose body content that cannot be assigned to a positional slot is an error. 
    If a slot receives no content, it expands to an empty string.
    Repeated slots, such as two occurrences of [codesnippet:[$slot(1)]], will replay the same content in both places.

    Because [codesnippet:$children(..)] only applies to direct child templates, nested helpers can scope row and cell wrappers independently.

    A child template is treated as an opaque contribution after it has been selected as a direct child.
    The parent [codesnippet:$children(..)] wrapper must not descend into that child and wrap its nested templates.
    If nested templates need their own wrappers, the child template should declare its own [codesnippet:$children(..)] directive.

    [codeblock, $code("bst"):
        # table = [$children([:<tr>[$slot]</tr>]):
            <table style="[$slot("style")]">
                [$slot]
            </table>
        ]

        # row = [$children([:<td>[$slot]</td>]):
            [$slot]
        ]

        [table:
            [row:
                [: Type]
                [: Description with [codesnippet:inline helper] content]
            ]

            [row:
                [: Float]
                [: 64-bit floating point number]
            ]
        ]
    ]

    In that example, [codesnippet:table] wraps each direct [codesnippet:row] child in [codesnippet:<tr>].
    Then [codesnippet:row] wraps each direct cell child in [codesnippet:<td>].
    The nested [codesnippet:codesnippet] helper inside a cell remains inline content inside that cell; it does not become another table cell.

    ## Template control flow examples
    [codeblock, $code("bst"):
        [head_of_element: content of element that renders as text (markdown / metadata / alt tag / code ..etc)]

        [ loop text_array |text|:
            [text]
        ]

        [if not text.isBlank():
            [ text: plus some extra text in the same element as the variable text ]
        ]
    ]

    ## Errors
    If an error is added to a template at runtime, it will be displayed in the console, rather than injected into the template. 
]
