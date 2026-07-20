import @./backend-lowering {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "17. Backend lowering"
page_description #= "How target backends lower validated semantic inputs."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / 17. Backend lowering

    # Turning HIR into JavaScript and Wasm

    Lowerers translate validated HIR into target code without reopening source or rewriting language meaning.
]

#[section, reference_content]

#[section, $md:
    @../ (Compiler design) / @../target-planning-and-validation (Previous) / @../html-assembly-and-output (Next)
]
