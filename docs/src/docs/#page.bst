import @html {center, img, large}
import @styles/docs {navbar, title, section, palette, codeblock, theme_head}

# page_title = "Documentation"
# page_description = "Beanstalk documentation index."
# page_head = theme_head

#[navbar]
#[title, center: THE DOCS]

#[section, $markdown:
    [large: Welcome to the Beanstalk documentation] 
    [$todo: 
        [$img, @assets/seedling_icon.png: Beanstalk logo]
    ]

    This website is built using Beanstalk's built in HTML project builder, and all the content you see here is written in Beanstalk code.

    You can find the source code for this website on @https://github.com/nyejames/beanstalk/tree/main/docs (Github).

    ## Documentation
    These pages document the current language surface.
    See the @./progress (implementation matrix) for the current implementation and coverage status.

    ### Introduction
    - @./language-overview (Language Basics)
    - @./getting-started (Getting Started)
    - @./project-structure (The shape of Beanstalk projects)

    ### Core Language Features
    - @./templates (Templates)
    - @./functions (Functions)
    - @./if-statements (If Statements)
    - @./loops (Loops)
    - @./collections (Collections)
    - @./structs (Structs)
    - @./choices (Choices)
    - @./pattern-matching (Pattern Matching)
    - @./aliases (Aliases)

    ### Future Design Drafts
    - @./errors (Error Handling)
    - @./reactivity (Reactivity)
    - @./async (Async)
    - @./generics (Generics)
    - @./traits (Traits)
]
