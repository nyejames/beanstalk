import @lib/html {format, center, img}
import @styles/docs {navbar, title, section, palette, table, row, header_row, codeblock, theme_head}

# page_title = "Documentation"
# page_description = "Beanstalk documentation index."
# page_head = theme_head

[navbar]
[title, center: THE DOCS]

[section, $markdown:
    Welcome to the Beanstalk documentation [img, @assets/seedling_icon.png: Beanstalk logo]

    This website is built using Beanstalk's built in HTML project builder, and all the content you see here is written in Beanstalk code.

    You can find the source code for this website on @https://github.com/nyejames/beanstalk/tree/main/docs (Github).

    ## Documentation
    The documentation is currently being written in Beanstalk one file at a time.

    - @./basics (Basics)
    - @./functions (Functions)
    - @./templates (Templates)
]
