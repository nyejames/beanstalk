import @html {center}
import @styles/docs {navbar, title, section, codeblock, theme_head}

page_title #= "Beandown"
page_description #= "Markdown-first .bd content files for HTML projects."
page_head #= theme_head

#[navbar]
#[title, center: BEANDOWN]

#[section, $markdown:
    Beandown files are Beanstalk-aware markdown-first content files. They use the same markdown
    flavour as string templates and can contain nested templates as if the file were a template body.

    Beandown files use the `.bd` extension for markdown-first content fragments in HTML projects.
    They are helpers for `.bst` pages and modules, not pages themselves.
    Use @./markdown for plain `.md` files that should be rendered as raw Markdown with no Beanstalk syntax.

    A `.bd` file is compiled as if its whole body were the body of an implicit compile-time markdown template:

    [codeblock, $code("bst"):
        content #String = [$markdown:
            ...entire .bd file body...
        ]
    ]

    The generated `content` constant is the only public surface of a Beandown file.
]

#[section, $markdown:
    ## Authoring a Beandown file

    Write markdown and nested Beanstalk templates directly in the `.bd` file:

    [codeblock, $code("bst"):
        # docs/intro.bd

        # Getting started

        [p:
            This paragraph is styled by the HTML builder's @html constants.
        ]
    ]

    Beandown starts inside a template body, so `--`, backticks, and backslashes are normal text.
    It also uses the same `$markdown` inline-code rules as `.bst` markdown templates.

    Literal template delimiters use normal string insertion:

    [codeblock, $code("bst"):
        ["["]
        ["]"]
        ["[literal]"]
    ]
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

    Do not include the `.bd` extension in the import path.
    `import @docs/intro.bd` is rejected; use `import @docs/intro`.
]

#[section, $markdown:
    ## Scope inside .bd

    Beandown bodies have a deliberately small compile-time scope:

    - exported compile-time constants and const records from `@html`, visible flat;
    - exported compile-time constants and const records from the same-directory `#mod.bst`, when present.

    Same-directory facade constants override `@html` constants with the same name.
    Functions, types, traits, methods, runtime bindings, runtime HTML APIs, and the generated self `content` constant are not visible.

    Keep richer composition in `.bst`.
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

    - `.bd` files are never entries, module roots, project types, or config files.
    - They do not support imports, declarations, frontmatter, metadata blocks, summaries, table-of-contents extraction, or raw-source retention.
    - They do not produce standalone HTML files or output folders.
    - A body must fully fold at compile time.

    Use `.bst` for pages, imports, functions, types, and content composition.
]
