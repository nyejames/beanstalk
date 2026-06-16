import @html {center}
import @styles/docs {navbar, title, section, codeblock, theme_head}

page_title #= "Core Prelude"
page_description #= "The small core surface available without explicit imports."
page_head #= theme_head

#[navbar]
#[title, center: CORE PRELUDE]

#[section, $markdown:
    The prelude is the tiny set of names available without an explicit import.
    It keeps common output and builtin error handling convenient without making every core library global.

    The `io` namespace is available in normal source files and exposes Core IO helpers such as `io.line(...)`.
    Compiler-owned `Error` is also available for `Error!` flows.

    [codeblock, $code("bst"):
        io.line("Hello from the prelude")
    ]

    Most core libraries still require imports:

    [codeblock, $code("bst"):
        import @core/math {sin}
        import @core/time {mark_now}
    ]
]
