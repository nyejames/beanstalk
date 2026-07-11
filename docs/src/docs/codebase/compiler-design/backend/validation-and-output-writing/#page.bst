import @./validation-and-output-writing {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Backend validation and output writing"
page_description #= "How Beanstalk validates target contracts and keeps output writing central."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Backend handoff) / Backend validation and output writing

    # Backend validation and output writing

    Validation runs before lowering so lowerers don't discover unsupported features late. Output writing stays in the build system so backends stay pure with respect to project state.
]

#[section, reference_content]
