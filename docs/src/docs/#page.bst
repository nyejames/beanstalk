import @lib/html/{format, center, img}
import @styles/docs/{navbar, footer, title, section, header, body, palette, table, row, header_row, codeblock}

#[header]
#[body:
[navbar]
[title, center: THE DOCS]

[section, $markdown:
    Welcome to the Beanstalk documentation [img: [$insert("src"): /assets/seedling_icon.png] Beanstalk logo]

    This website is built using Beanstalk's built in HTML project builder, and all the content you see here is written in Beanstalk code.

    You can find the source code for this website on @https://github.com/nyejames/beanstalk/tree/main/docs (Github).
]

] -- end of body
