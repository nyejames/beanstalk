import @./type-identity-and-coercion {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Type identity and coercion"
page_description #= "How Beanstalk decides what a type means and where contextual coercion applies."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / Type identity and coercion

    # Type identity and coercion

    `TypeEnvironment` owns semantic identity. `DataType` is syntax. Keeping that line sharp protects every later stage from comparing parse shapes instead of real types.
]

#[section, reference_content]
