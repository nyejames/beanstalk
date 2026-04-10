import @lib/html {format, style}
import @styles/docs {navbar, title, section, palette, table, data, codeblock, codesnippet, theme_head}

# page_title = "If Statements"
# page_description = "If statements in Beanstalk"
# page_head = theme_head

#[navbar]
#[title, style.center: IF STATEMENTS]

#[section, $markdown:
    ## Basic Syntax
    [codeblock, $code("bst"):
        v = 1

        if v is not 1:
            io("nope")
        else
            io("yep")
        ;

        io(msg) -- prints "nope"
    ]
]