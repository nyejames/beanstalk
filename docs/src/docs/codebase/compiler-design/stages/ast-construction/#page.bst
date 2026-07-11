import @./ast-construction {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Stage 4: AST construction"
page_description #= "Where source declarations become typed AST and HIR-ready structures."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Pipeline stages) / Stage 4: AST construction

    # Stage 4: AST construction

    AST is where most frontend semantics become real. It consumes sorted headers, resolves declarations and prepares clean input for HIR.
]

#[section, reference_content]
