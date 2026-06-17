import @html {center}
import @styles/docs {navbar, title, section, styled_table, header, data, codeblock, theme_head}

page_title #= "Markdown Imports"
page_description #= "Plain .md content imports for HTML projects."
page_head #= theme_head

#[navbar]
#[title, center: MARKDOWN IMPORTS]

#[section, $markdown:
    Plain Markdown files use the `.md` extension for raw Markdown content fragments in HTML projects.
    They are helpers for `.bst` pages and modules, not pages themselves.

    A `.md` file is rendered to HTML at compile time and exposed as one generated constant:

    [codeblock, $code("bst"):
        content #String = "<rendered html>"
    ]

    Use `.md` when the file should be plain Markdown with no Beanstalk syntax. Use @./beandown
    when the content needs nested templates, `$markdown`, or same-directory facade constants.
]

#[section, $markdown:
    ## Importing a Markdown file

    Project shape:

    [codeblock, $code("text"):
        src/
        ├── #page.bst
        └── docs/
            └── intro.md
    ]

    Markdown source:

    [codeblock, $code("text"):
        # Welcome

        This is **plain Markdown**.
    ]

    Beanstalk page:

    [codeblock, $code("bst"):
        import @docs/intro

        [: [intro.content] ]
    ]

    Grouped aliases work too:

    [codeblock, $code("bst"):
        import @docs/intro {
            content as intro_html,
        }

        [: [intro_html] ]
    ]

    Do not include the `.md` extension in the import path.
    `import @docs/intro.md` is rejected; use `import @docs/intro`.
]

#[section, $markdown:
    ## What .md does not do

    Plain Markdown files do not parse Beanstalk source syntax.

    - `--` is Markdown text, not a Beanstalk comment.
    - `$100` is text, not a template directive.
    - `["[not_a_template]"]` is text, not a Beanstalk template.
    - `["[: body]"]` is text, not a template body.

    `.md` files do not support Beanstalk imports, declarations, interpolation, templates,
    frontmatter, metadata blocks, summaries, table-of-contents extraction, or raw-source retention.
    They also do not see same-directory `#mod.bst` constants or `@html` constants.
]

#[section, $markdown:
    ## HTML, links, and assets

    Raw HTML is preserved in V1. There is no sanitizer or raw-HTML policy key in this feature.

    Markdown links and images render their authored paths literally:

    [codeblock, $code("text"):
        [Next](./next.md)
        ![Diagram](./diagram.png)
    ]

    V1 does not rewrite those paths and does not copy linked or image assets. Use normal Beanstalk
    asset paths when a file should participate in tracked-asset output.
]

#[section, $markdown:
    ## .md and .bd compared

    [styled_table:
        [header: [: Feature] [: `.md`] [: `.bd`] ]

        [data:
            [: Parser]
            [: CommonMark-style dependency]
            [: Beanstalk `$markdown`]
        ]

        [data:
            [: Beanstalk syntax]
            [: No]
            [: Yes, template body rules]
        ]

        [data:
            [: Generated member]
            [: `content #String`]
            [: `content #String`]
        ]

        [data:
            [: Same-directory facade constants]
            [: No]
            [: Yes]
        ]

        [data:
            [: Frontmatter or metadata]
            [: No]
            [: No]
        ]

        [data:
            [: Raw HTML]
            [: Preserved V1]
            [: Through Beanstalk/template semantics]
        ]

        [data:
            [: Links and images]
            [: Literal output V1]
            [: Template-rendered output]
        ]
    ]
]
