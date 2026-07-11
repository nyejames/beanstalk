import @./expressions-statements-and-terminality {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Expressions, statements and terminality"
page_description #= "How source statements and expressions become typed AST."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (AST internals) / Expressions, statements and terminality

    # Expressions, statements and terminality

    AST body emission parses executable bodies against the completed environment. The goal is strict, local semantics.
]

#[section, reference_content]
