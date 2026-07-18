import @./dependency-sorting {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Stage 3: Local declaration ordering"
page_description #= "How Beanstalk topologically orders declarations inside one canonical module."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Compiler pipeline) / Stage 3: Local declaration ordering

    # Stage 3: Local declaration ordering

    Stage 3 orders declarations owned by one module. Provider modules were already ordered by the build-system graph.
]

#[section, reference_content]
