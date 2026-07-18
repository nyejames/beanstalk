import @./ast-construction {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Stage 4: AST semantics"
page_description #= "Where retained source structure becomes checked semantic meaning."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Compiler pipeline) / Stage 4: AST semantics

    # Stage 4: AST semantics

    AST resolves declarations, checks bodies, folds compile-time values and prepares explicit semantic input for HIR.
]

#[section, reference_content]
