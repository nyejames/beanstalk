import @./constants-and-folding {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Constants and folding"
page_description #= "How Beanstalk folds compile-time constants and shares semantics with runtime."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (AST internals) / Constants and folding

    # Constants and folding

    Constants are compile-time declarations. AST folds what must fold so compile-time and runtime behavior stay aligned.
]

#[section, reference_content]
