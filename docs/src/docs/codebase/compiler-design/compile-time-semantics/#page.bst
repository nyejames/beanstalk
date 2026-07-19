import @./compile-time-semantics {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "08. Compile-time semantics"
page_description #= "How AST-owned static work becomes concrete data and requests."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / 08. Compile-time semantics

    # Constants, generics, traits and casts before HIR

    Outline for the later educational writing pass. The canonical architecture remains in the linked authority documents.
]

#[section, reference_content]

#[section, $md:
    @../ (Compiler design) / @../ast-semantics-and-types (Previous) / @../templates-and-tir (Next)
]

