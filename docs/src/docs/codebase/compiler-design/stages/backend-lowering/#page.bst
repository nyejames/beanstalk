import @./backend-lowering {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Stage 7: Backend lowering"
page_description #= "How Beanstalk hands compiled modules to builders and emits target artifacts."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Pipeline stages) / Stage 7: Backend lowering

    # Stage 7: Backend lowering

    Backend lowering belongs to project builders after frontend compilation. Lowerers generate target-specific code from validated HIR.
]

#[section, reference_content]
