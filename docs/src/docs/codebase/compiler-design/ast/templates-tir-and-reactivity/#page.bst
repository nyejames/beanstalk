import @./templates-tir-and-reactivity {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Templates, TIR and reactivity"
page_description #= "How Beanstalk handles templates and reactive subscriptions before HIR."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (AST internals) / Templates, TIR and reactivity

    # Templates, TIR and reactivity

    Templates are frontend-owned until AST finishes preparing them. HIR only lowers final runtime templates.
]

#[section, reference_content]
