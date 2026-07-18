import @./backend-lowering {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Target planning and backend lowering"
page_description #= "How explicit roots, assignments and validated HIR become target artefacts."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Compiler pipeline) / Target planning and backend lowering

    # Target planning and backend lowering

    The build system plans artefacts, the compiler validates target contracts and lowerers generate target-specific code.
]

#[section, reference_content]
