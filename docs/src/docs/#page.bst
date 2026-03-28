import @lib/html {format, style, img, large}
import @styles/docs {navbar, title, section, palette, codeblock, theme_head}

# page_title = "Documentation"
# page_description = "Beanstalk documentation index."
# page_head = theme_head

#[navbar]
#[title, style.center: THE DOCS]

#[section, $markdown:
    [large: Welcome to the Beanstalk documentation] 
    [$todo: 
        [$img, @assets/seedling_icon.png: Beanstalk logo]
    ]
    

    This website is built using Beanstalk's built in HTML project builder, and all the content you see here is written in Beanstalk code.

    You can find the source code for this website on @https://github.com/nyejames/beanstalk/tree/main/docs (Github).

    ## Documentation
    The documentation is currently being written in Beanstalk one file at a time.

    - @./basics (Basics)
    - @./functions (Functions)
    - @./templates (Templates)
]
