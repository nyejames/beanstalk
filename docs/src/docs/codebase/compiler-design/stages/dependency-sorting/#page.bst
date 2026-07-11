import @./dependency-sorting {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Stage 3: Dependency sorting"
page_description #= "How Beanstalk orders top-level declarations and reports dependency cycles."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Pipeline stages) / Stage 3: Dependency sorting

    # Stage 3: Dependency sorting

    Dependency sorting makes declarations linearly consumable by AST. It never looks inside executable bodies.
]

#[section, reference_content]
