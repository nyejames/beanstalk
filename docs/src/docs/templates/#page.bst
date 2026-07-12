import @html {center}
import @styles/docs {navbar, title, section, palette, styled_table, header, codeblock, theme_head}

page_title #= "Templates"
page_description #= "Template syntax, slots, directives, and formatting in Beanstalk."
page_head #= theme_head

#[navbar]
#[title, center: TEMPLATES]

#[section, $md:
    Beanstalk has unique, superpowered templating strings. 

    Templates can be used to generate complex text, Markdown, HTML, or any kind of string-based structure you want. 
    They fold at compile time or lower to runtime string construction, with structured slots, wrappers and formatter directives.
    They replace the same underpowered string formatter functions/macros languages have been stuck using forever.
    
    Templates use `["[]"]` exclusively - not to be confused with collections `{}``.

    These are the only way to create mutable strings, as double quotes are only used for string slices.

    The main purpose of a template is to generate either complex text content, markdown, HTML or just regular formatted strings.

    Templates have two sections separated by a colon:
    - Head
    - Body

    This example creates the string ` "head body"`
    [codeblock, $code("bst"):
        ["head": body]
    ]

    Templates are scoped using square brackets and can be passed special types called Styles to change their behavior, content and styling.
    
    Templates are either folded to strings at compile time, or become functions that return strings at runtime. 

    ## Structure
    - Head and body separated by `:`
    - Variable capture from the surrounding scope

    Templates unlock the full power of Beanstalk's HTML / CSS generation capabilities.

    ## Styles
    Templates can be used to build complex UI components. They can use slots to insert content from other templates and have **style metadata** attached to them.

    A template's style is defined in the **template head** using ` $` directives.
    `$` introduces **compiler-handled directives** (so they don't collide with normal variables and can be extended in the future), such as formatter-like built-ins, precedence controls, and default child templates that are automatically applied to direct child templates.

    Directive availability is frontend-registry based:
    - Frontend built-ins are available by default (`$md`, `$code`, `$raw`, `$slot`, etc.).
    - Project builders can register additional project-specific directives using the same `$name` syntax. In the HTML project, that includes `$html`, `$css`, and `$escape_html`.
    - Unknown directives fail as syntax/rule errors unless they are registered.

    [codeblock, $code("bst"):
        -- Define a template style
        [$md, $children([: All children start with this prefix! ]):
            # Hello
            This template is parsed as markdown.

            @example.com (Here is a link!) using this custom markdown flavour.

            [$todo: write some more info!]

            [: This child is prefixed!]
        ]
    ]

    **Frontend Built-in Style Directives**

    - `$slot` / `$insert(..)` - See slots below!
    - `$fresh`              - Opts this child template out of wrappers applied by the immediate parent's `$children(..)` directive
    - `$md`           - Parses the template bodies with a custom flavour of Markdown
    - `$code`               - Highlights code blocks using the compiler's built-in formatter
    - `$raw`                - Parses the template body as a raw string (no escaping, no formatting)
    - `$note / $todo`       - Comments (ignored by final output)
    - `$doc`                - Turns the template into a documentation comment
    - `$children(..)`       - Accepts a template (or string slice) that will be applied only to this template's direct child templates

    **HTML Project Directives**

    - `$html`               - Parses the template body as raw HTML (no escaping)
    - `$css`                - Provides some basic warnings for malformed CSS
    - `$escape_html`        - Escapes HTML-sensitive characters in the template body

    Formatting directives do not automatically flow into nested child templates.
    If a child template should keep using a formatter such as `$md`, redeclare it in that child template's head.
    Beandown `.bd` files are the exception: nested templates with no explicit directive default to `$md`,
    and any explicit directive on that nested template overrides the Beandown default.

    `$fresh` is per-child and only affects wrapper application from the immediate parent. Siblings without `$fresh` still receive the parent wrappers:

    [codeblock, $code("bst"):
        list #= [$children([:<li>[$slot]</li>]):
            <ul>
                [$slot]
            </ul>
        ]

        [list:
            [: one ]
            [$fresh: [: two ]]
        ]
    ]

    In this example, `one` is wrapped with `<li>...</li>`, while `two` opts out and is rendered without the parent `$children(..)` wrapper.

    ## Markdown inline code

    `$md` supports small inline code spans with paired isolated single backticks on one line.
    Inline code renders as a normal HTML `code` element, preserves whitespace, escapes authored HTML-sensitive characters, and does not parse emphasis or links inside the span.

    [codeblock, $code("bst"):
        [$md:
            Use `copy` when you need an explicit copy.
            Write literal template syntax as `["[$slot]"]`.
        ]
    ]

    Unmatched backticks and repeated backtick runs render literally.
    Triple-backtick fences, variable-length code-span delimiters, multiline code spans, and markdown-level backtick escaping are not part of Beanstalk markdown.
    Use `$code(...)` for larger code examples.

    ## Slots

    Template slots let one template receive content from another template. The default slot is written as `["[$slot]"]` and marks where the main body content should be inserted.

    Named slots can also be declared with `["[$slot(\"name\")]"]`. These allow helper templates to insert content into a specific part of another template using `$insert("name")`.

    Positional slots can be declared using positive integers, such as `["[$slot(1)]"]`, `["[$slot(2)]"]`, etc. Loose contributions (those not explicitly targetting a named slot) are assigned to positional slots first, in ascending numeric order. Any remaining loose contributions go to the default slot if it exists.

    [codeblock, $code("bst"):
        img = [:
            <img src="[$slot(1)]" alt="[$slot]">
        ]

        [img, "logo.png": Site logo]
    ]

    In this example, `"logo.png"` fills the first positional slot `["[$slot(1)]"]`, and `"Site logo"` fills the default `$slot`.

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

    In this example, `blue` inserts `color: blue;` into the `$slot("style")` slot of `title`, while `Hello world` is inserted into the default `$slot`.

    If a template has named or positional slots but no default slot, any loose body content that cannot be assigned to a positional slot is an error. 
    If a slot receives no content, it expands to an empty string.
    Repeated slots, such as two occurrences of `["[$slot(1)]"]`, will replay the same content in both places.

    Slot applications may contain runtime-producing content. Runtime-capable
    templates support default, named, positional, and loose contributions from
    template `if` branches and template `loop` bodies.
    Valid runtime slot applications are routed during AST preparation, then
    lowered through ordinary runtime string accumulators. Missing slots still
    render as empty strings, and repeated slots replay the same accumulated
    contribution.

    [codeblock, $code("bst"):
        card = [:
            <h1>[$slot("title")]</h1>
            <section>[$slot]</section>
        ]

        [card:
            [$insert("title"):
                [if maybe_title is |title|:
                    [title]
                [else]
                    Untitled
                ]
            ]

            [loop items |item|:
                [item]
            ]
        ]
    ]

    Because `$children(..)` only applies to direct child templates, nested helpers can scope row and cell wrappers independently.

    A child template is treated as an opaque contribution after it has been selected as a direct child.
    The parent `$children(..)` wrapper must not descend into that child and wrap its nested templates.
    If nested templates need their own wrappers, the child template should declare its own `$children(..)` directive.

    [codeblock, $code("bst"):
        table #= [$children([:<tr>[$slot]</tr>]):
            <table style="[$slot("style")]">
                [$slot]
            </table>
        ]

        row #= [$children([:<td>[$slot]</td>]):
            [$slot]
        ]

        [styled_table:
            [row:
                [: Type]
                [: Description with [$html:<code>inline helper</code>] content]
            ]

            [row:
                [: Float]
                [: 64-bit floating point number]
            ]
        ]
    ]

    In that example, `table` wraps each direct `row` child in `<tr>`.
    Then `row` wraps each direct cell child in `<td>`.
    The nested inline `code` element inside a cell remains inline content inside that cell; it does not become another table cell.

    ## Template head suffix control flow

    Template control flow is written as the final suffix in the template head,
    immediately before the body colon. If the head already contains a value,
    helper, or directive, put a comma before `if` or `loop`.

    [codeblock, $code("bst"):
        [if show:
            Visible
        ]

        [card, if show:
            Visible inside card
        ]
    ]

    Boolean `if` conditions choose one branch lazily at runtime.
    `["[else if ...]"]` adds more branches in source order and uses
    the same Bool and option-present selectors as the initial `if`.
    `["[else]"]` is optional, must be standalone, and binds to the nearest
    active template `if`.

    [codeblock, $code("bst"):
        show_summary = false
        show_details = true

        [if show_summary:
            Summary
        [else if show_details]
            Details
        [else]
            Empty
        ]
    ]

    Option-present captures use the normal `is |name|` form. The
    captured value is visible only inside the selected branch.

    [codeblock, $code("bst"):
        maybe_name String? = none
        maybe_backup String? = "Ada"

        [if maybe_name is |name|:
            Hello [name]
        [else if maybe_backup is |backup|]
            Hello [backup]
        [else]
            Hello guest
        ]
    ]

    Full pattern-match branch chains belong to ordinary statement/value
    `if value is:` blocks, not template heads.

    Template loops support conditional loops, collection loops, and numeric range
    loops using the same header syntax as statement loops. Conditional template
    loops do not take bindings. Collection and range loops can use a second
    binding for the zero-based index. Loops concatenate iterations directly;
    there is no implicit separator. Standalone `["[break]"]` and
    `["[continue]"]` sentinels are valid inside template loop bodies,
    including nested template `if`, `["[else if ...]"]`,
    and `["[else]"]` bodies. They target the nearest active template
    loop and are structural control signals, not rendered output.

    [codeblock, $code("bst"):
        [loop has_next():
            [next_item()]
        ]

        [loop items |item|:
            [item]
        ]

        [loop items |item, index|:
            [index]: [item]
        ]

        [loop 0 to 10 |i|:
            [i]
        ]

        [loop items |item|:
            [if item.skip:
                [continue]
            ]
            [if item.done:
                [break]
            ]
            [item]
        ]
    ]

    Head values and wrappers apply to the selected or generated output. For a
    loop, the owning head wraps the whole aggregate once. Put per-iteration
    wrappers inside the loop body.

    [codeblock, $code("bst"):
        [list, loop items |item|:
            [row: [item]]
        ]
    ]

    A false `if` with no `["[else]"]` produces structural
    no-output. A false conditional loop and zero-iteration collection or range
    loop also produce no-output. In those cases, shared head wrappers and parent
    `$children(..)` wrappers are skipped. A selected branch with an
    empty body is still selected, so its head wrappers may render an empty wrapper.
    A loop iteration that reaches `["[break]"]` or
    `["[continue]"]` before any output also does not count as emitted.
    Output before a loop-control sentinel is preserved.

    Top-level `["#[if ...:]"]` and `["#[loop ...:]"]` fragments
    must fully fold at compile time. Const-required template `if`
    forms validate every branch body, even when one path is inactive.
    Const-required conditional template loops fold to no-output when their
    condition is compile-time `false`. Compile-time `true`
    and unknown/runtime conditional-loop conditions are rejected because the
    compiler does not perform template loop termination analysis. Const range
    and collection template loops can fold structural `["[break]"]`
    and `["[continue]"]`, and use the
    `template_const_loop_iteration_limit` project config guard. The
    default is `10_000` iterations per const template loop, and the
    configured value must be a positive folded `Int` no greater than
    `1_000_000`.

    Runtime template control flow stays lazy. Only the selected `if`
    branch is evaluated. Range and collection loop sources are evaluated once
    before iteration; conditional loop conditions are evaluated before every
    iteration.
    Expression-level `!` inside selected branch or loop content
    propagates from the enclosing function.

    Runtime slot applications are valid inside template control flow after
    normal slot routing. Escaped helper artifacts that still leave unresolved
    `["[$slot]"]` or `$insert(...)` output inside runtime
    control-flow bodies are invalid template structure.
    Runtime slot applications appended inside template loops follow the same
    nearest-template-loop `["[break]"]` / `["[continue]"]`
    rules as direct loop body content.

]
