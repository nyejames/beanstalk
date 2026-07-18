import @./project-structure {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Build-system Stage 0"
page_description #= "How project config, source ownership and dependency graphs become compiler inputs."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Compiler pipeline) / Build-system Stage 0

    # Build-system Stage 0

    Stage 0 prepares deterministic project or package graphs and compiler inputs. It is build orchestration rather than semantic frontend compilation.
]

#[section, reference_content]
