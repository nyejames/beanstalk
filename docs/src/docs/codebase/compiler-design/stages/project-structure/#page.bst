import @./project-structure {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Stage 0: Project structure"
page_description #= "How Beanstalk discovers files, parses config and prepares module inputs."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Pipeline stages) / Stage 0: Project structure

    # Stage 0: Project structure

    Stage 0 finds the files and config the frontend needs. It stops before semantic compilation so the build system and frontend stay separate.
]

#[section, reference_content]
