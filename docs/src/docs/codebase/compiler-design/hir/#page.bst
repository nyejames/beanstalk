import @./hir {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "11. HIR"
page_description #= "How typed meaning becomes explicit backend-facing control flow."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / 11. HIR

    # Lowering meaning into explicit control flow

    Outline for the later educational writing pass. The canonical architecture remains in the linked authority documents.
]

#[section, reference_content]

#[section, $md:
    @../ (Compiler design) / @../diagnostics (Previous) / @../memory-management-and-gc (Next)
]

