import @./access-and-aliasing {content as reference_content}
import @styles/docs {navbar, section, theme_head}

page_title #= "Access and aliasing"
page_description #= "How Beanstalk shares, copies and requests exclusive access to values."
page_head #= theme_head

#[navbar]

#[section, $md:
    @../ (Memory management) / Access and aliasing

    # Access and aliasing

    Existing values are shared by default. Copies are explicit. `~` requests exclusive access to a mutable place.
]

#[section, reference_content]
