import @./imports-libraries-and-external-packages {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Imports, libraries and external packages"
page_description #= "How Beanstalk resolves imports, facades and builder-provided external packages."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / Imports, libraries and external packages

    # Imports, libraries and external packages

    Imports resolve before body semantics. Facades define public API surfaces. External packages give builders typed virtual symbols without pretending they are source files.
]

#[section, reference_content]
