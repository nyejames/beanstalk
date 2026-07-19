import @./diagnostics {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "10. Diagnostics"
page_description #= "How compiler failures carry semantic context from owner to renderer."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / 10. Diagnostics

    # Compiler errors as structured data

    Outline for the later educational writing pass. The canonical architecture remains in the linked authority documents.
]

#[section, reference_content]

#[section, $md:
    @../ (Compiler design) / @../templates-and-tir (Previous) / @../hir (Next)
]

