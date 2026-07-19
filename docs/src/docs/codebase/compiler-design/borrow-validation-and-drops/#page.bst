import @./borrow-validation-and-drops {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "13. Borrow validation and drops"
page_description #= "How Beanstalk analyses HIR for safe access and inferred ownership."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / 13. Borrow validation and drops

    # Borrow checking, race prevention and inferred destruction

    Outline for the later educational writing pass. The canonical architecture remains in the linked authority documents.
]

#[section, reference_content]

#[section, $md:
    @../ (Compiler design) / @../memory-management-and-gc (Previous) / @../module-artefacts-and-reuse (Next)
]

