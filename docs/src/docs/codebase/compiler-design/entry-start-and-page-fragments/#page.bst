import @./entry-start-and-page-fragments {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Entry activation and page fragments"
page_description #= "How compiled dormant root work becomes one selected entry artefact."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / Entry activation and page fragments

    # Entry activation and page fragments

    Normal modules compile dormant root work once. Entry assembly later activates one selected module and merges its compile-time and runtime page fragments.
]

#[section, reference_content]
