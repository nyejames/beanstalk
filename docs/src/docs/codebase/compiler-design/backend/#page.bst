import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Target planning and backend lowering"
page_description #= "Supplementary explanations of target validation, project assembly and code generation."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / Target planning and backend lowering

    # Target planning and backend lowering

    Build orchestration selects roots and targets. The compiler validates the completed plan. Lowerers generate target code from explicit validated semantic inputs.
]

#[section, $md:
    ## Explore

    - @./validation-and-output-writing (Target validation and output ownership)
    - @./html-project-assembly (HTML project assembly)
    - @./js-lowering (JavaScript lowering)
    - @./wasm-lowering (Wasm lowering)
    - @./external-js-and-runtime-assets (External JavaScript and runtime assets)
    - @./tracked-assets (Tracked assets)
]
