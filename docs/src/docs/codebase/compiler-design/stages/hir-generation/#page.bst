import @./hir-generation {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Stage 5: HIR generation"
page_description #= "How Beanstalk lowers typed AST into explicit backend-facing IR."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Pipeline stages) / Stage 5: HIR generation

    # Stage 5: HIR generation

    HIR makes control flow explicit enough for borrow validation and backend lowering. It isn't a source-level AST with a new name.
]

#[section, reference_content]
