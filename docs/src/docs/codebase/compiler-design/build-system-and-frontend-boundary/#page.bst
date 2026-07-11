import @./build-system-and-frontend-boundary {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Build-system and frontend boundary"
page_description #= "Where the build system ends and the compiler frontend begins."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / Build-system and frontend boundary

    # Build-system and frontend boundary

    The build system prepares inputs. The frontend compiles them. Keeping the line sharp stops backends from rediscovering source files and the frontend from guessing builder policy.
]

#[section, reference_content]
