import @html {center}
import @styles/docs {navbar, title, section, codeblock, codesnippet, theme_head}

page_title #= "Core IO"
page_description #= "Standard output basics in Beanstalk."
page_head #= theme_head

#[navbar]
#[title, center: CORE IO]

#[section, $markdown:
    [codesnippet:io(...)] writes to standard output.
    It is part of the prelude, so normal source files can call it without importing [codesnippet:@core/io].

    [codeblock, $code("bst"):
        io("Hello")

        name = "Beanstalk"
        io([:Hello, [name]])
    ]

    Strings and templates are accepted at the output boundary.
    Other values should be converted through the normal language paths that are valid for templates or strings.
]
