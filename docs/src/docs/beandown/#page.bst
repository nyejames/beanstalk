import @html {center}
import @styles/docs {navbar, title, section, codeblock, codesnippet, theme_head}

page_title #= "Beandown"
page_description #= "Markdown-first .bd content files for HTML projects."
page_head #= theme_head

#[navbar]
#[title, center: BEANDOWN]

#[section, $markdown:
    Beandown files use the [codesnippet:.bd] extension for markdown-first content fragments in HTML projects.
    They are helpers for [codesnippet:.bst] pages and modules, not pages themselves.

    A [codesnippet:.bd] file is compiled as if its whole body were the body of an implicit compile-time markdown template:

    [codeblock, $code("bst"):
        content #String = [$markdown:
            ...entire .bd file body...
        ]
    ]

    The generated [codesnippet:content] constant is the only public surface of a Beandown file.
]

#[section, $markdown:
    ## Authoring a Beandown file

    Write markdown and nested Beanstalk templates directly in the [codesnippet:.bd] file:

    [codeblock, $code("bst"):
        # docs/intro.bd

        # Getting started

        [p:
            This paragraph is styled by the HTML builder's @html constants.
        ]
    ]

    Beandown starts inside a template body, so [codesnippet:--] is normal text.
    Escape a literal closing bracket as [codesnippet, "\\]"].
]

#[section, $markdown:
    ## Importing from .bst

    Import Beandown content with normal extensionless source imports:

    [codeblock, $code("bst"):
        import @docs/intro

        [: [intro.content] ]
    ]

    Grouped aliases work too:

    [codeblock, $code("bst"):
        import @docs/intro {
            content as intro_content,
        }

        [: [intro_content] ]
    ]

    Do not include the [codesnippet:.bd] extension in the import path.
    [codesnippet:import @docs/intro.bd] is rejected; use [codesnippet:import @docs/intro].
]

#[section, $markdown:
    ## Scope inside .bd

    Beandown bodies have a deliberately small compile-time scope:

    - exported compile-time constants and const records from [codesnippet:@html], visible flat;
    - exported compile-time constants and const records from the same-directory [codesnippet:#mod.bst], when present.

    Same-directory facade constants override [codesnippet:@html] constants with the same name.
    Functions, types, traits, methods, runtime bindings, runtime HTML APIs, and the generated self [codesnippet:content] constant are not visible.

    Keep richer composition in [codesnippet:.bst].
]

#[section, $markdown:
    ## Facade re-exports

    A module facade can explicitly re-export Beandown content:

    [codeblock, $code("bst"):
        -- docs/#mod.bst
        export @./intro {
            content as intro,
        }
    ]

    Another page can then import the public facade name:

    [codeblock, $code("bst"):
        import @docs { intro }

        [: [intro] ]
    ]
]

#[section, $markdown:
    ## Boundaries

    Beandown is intentionally small.

    - [codesnippet:.bd] files are never entries, module roots, project types, or config files.
    - They do not support imports, declarations, frontmatter, metadata blocks, summaries, table-of-contents extraction, or raw-source retention.
    - They do not produce standalone HTML files or output folders.
    - A body must fully fold at compile time.

    Use [codesnippet:.bst] for pages, imports, functions, types, and content composition.
]
