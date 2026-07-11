import @./environment-and-type-resolution {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "AST environment and type resolution"
page_description #= "How declaration shells become semantic type identities and visibility."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (AST internals) / AST environment and type resolution

    # AST environment and type resolution

    This is where type identity becomes real. Headers provide shape and order. AST resolves meaning.
]

#[section, reference_content]
