import @./target-planning-and-validation {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "16. Target planning and validation"
page_description #= "How Beanstalk plans and validates mixed JavaScript and Wasm targets."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / 16. Target planning and validation

    # Deciding where code can run

    Outline for the later educational writing pass. The canonical architecture remains in the linked authority documents.
]

#[section, reference_content]

#[section, $md:
    @../ (Compiler design) / @../linking-entries-and-reachability (Previous) / @../backend-lowering (Next)
]

