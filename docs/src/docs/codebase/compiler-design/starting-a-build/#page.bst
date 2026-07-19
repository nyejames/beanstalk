import @./starting-a-build {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "03. Starting a build"
page_description #= "How command selection and project config prepare a Beanstalk build."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / 03. Starting a build

    # A build starts before source compilation

    Outline for the later educational writing pass. The canonical architecture remains in the linked authority documents.
]

#[section, reference_content]

#[section, $md:
    @../ (Compiler design) / @../beanstalk-design-choices (Previous) / @../project-graphs-and-modules (Next)
]

