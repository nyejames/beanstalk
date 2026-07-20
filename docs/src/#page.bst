import @styles/docs {navbar, section, theme_head, palette}

page_title #= "What is Beanstalk?"
page_description #= "Introduction to the Beanstalk language and toolchain."
page_head #= theme_head

#[navbar]
#[section, $md:
    # What is Beanstalk?
    Beanstalk is a new programming language and toolchain.

    [palette.highlight, $md: *This website was created using Beanstalk!*]

    Everything you see on this page, including the layout, styles, and even the text content 
    was written in Beanstalk files and generated using the compiler's built-in HTML project builder.

    The source code for this whole website can be found on @https://github.com/nyejames/beanstalk/tree/main/docs/src (Github).

    ## Documentation
    The @./docs (Alpha language docs) explains everything about the language, compiler and making Beanstalk projects.

    This compiler is in early alpha and is undergoing rapid development, so not everything is exactly up to date with the current compiler.
    [palette.highlight, $md: *You can already use beanstalk for web projects, but be aware of the early alpha status*]
]

export:
    import @styles/docs {
        Palette,
        theme_css,
        palette,
        theme_head,
        language_docs_css,
        language_theme_head,
        section,
        navbar,
        footer,
        header,
        styled_table,
        data,
        codeblock,
        title,
        doc_level,
        doc_pager,
        doc_pager_previous,
        doc_pager_next,
    }
;

