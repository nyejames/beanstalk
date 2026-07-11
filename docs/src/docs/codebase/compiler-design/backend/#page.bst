import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Backend handoff"
page_description #= "How validated HIR becomes project artifacts and target-specific code."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / Backend handoff

    # Backend handoff

    Backend work starts after frontend compilation. Validated HIR and companion metadata cross a strict boundary: the build system orchestrates, project builders decide artifact structure and lowerers emit target-specific code.
]

#[section, $md:
    ## The handoff

    - The build system owns compilation orchestration and final output writing
    - Project builders own routes, artifact sets and target selection
    - Backend validation checks whether the selected target can lower the reachable program
    - Backend lowerers turn validated HIR into JavaScript, Wasm or another target representation

    Backend code must not rediscover source syntax, import visibility, trait evidence or template structure.
]

#[section, $md:
    ## Explore backend internals

    - @./validation-and-output-writing (Backend validation and output writing): target contracts, output records and shared writing rules
    - @./html-project-assembly (HTML project assembly): routes, page fragments and project artifact planning
    - @./js-lowering (JavaScript lowering): runtime helpers and HIR-to-JavaScript emission
    - @./wasm-lowering (Wasm lowering): LIR, runtime contracts and binary emission
    - @./external-js-and-runtime-assets (External JavaScript and runtime assets): provider imports, glue and runtime modules
    - @./tracked-assets (Tracked assets): rendered paths, copied assets and output conflicts
]
