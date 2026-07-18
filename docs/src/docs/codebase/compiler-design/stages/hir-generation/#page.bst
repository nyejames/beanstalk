import @./hir-generation {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Stage 5: HIR and validation"
page_description #= "How typed AST becomes explicit validated backend-facing semantic IR."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Compiler pipeline) / Stage 5: HIR and validation

    # Stage 5: HIR and validation

    HIR makes executable semantics explicit enough for analysis and target lowering while non-executable compiler facts remain in separate artefact lanes.
]

#[section, reference_content]
