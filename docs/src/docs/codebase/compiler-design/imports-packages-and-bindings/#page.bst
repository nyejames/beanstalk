import @./imports-packages-and-bindings {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Imports, packages and bindings"
page_description #= "How Beanstalk resolves imports, module public surfaces and source-backed or binding-backed packages."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / Imports, packages and bindings

    # Imports, packages and bindings

    Imports resolve before body semantics. Module roots define public API surfaces. Binding-backed packages give builders typed virtual symbols without pretending they are source files.
]

#[section, reference_content]
