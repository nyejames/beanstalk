import @./templates-and-tir {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "09. Templates and TIR"
page_description #= "How Beanstalk preserves template structure before runtime lowering."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / 09. Templates and TIR

    # Why templates need their own temporary IR

    Outline for the later educational writing pass. The canonical architecture remains in the linked authority documents.
]

#[section, reference_content]

#[section, $md:
    @../ (Compiler design) / @../compile-time-semantics (Previous) / @../diagnostics (Next)
]

