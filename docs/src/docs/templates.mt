#import(@libs/html/basic)
#import(@styles/docs_styles)
#import(@./components)
[basic.page:

[docs_styles.Navbar]

[docs_styles.Header, basic.Center: [basic.Title: BEANSTALK TEMPLATES]]

[basic.Section:

Beanstalk has unique, superpowered templating strings.

These are the only way to create mutable strings, as double quotes are only used for string slices.

Templates have two sections, the head and the body, separated with a colon. 

Templates are scoped using square brackets and can be passed special types called Styles to change their behavior, content and styling. 

The main purpose of a template is to generate either text content, markdown, HTML, code or just formatted strings.

You can think of them in their most basic form as a function at runtime that returns a string, 
but they turn into regular strings at compile time if they identify that they can be folded and won't change at runtime.

In the future, templates **might** be extended to generate more than just strings, 
and become a small macro system (not sure if this is the right direction for the language yet).

#### The head section
Is used to:
- Convert literals and expressions dynamically to strings to be added into the template
- Wrap the template itself in strings that conditionally depend on the template's properties or parent template's properties

The head section is dynamic and coerses all primatives directly into strings.
However, more complex types (structs and other templates) can interact with templates in a more complex way *TODO*.

When you pass a template into another template via the head, it's content can be injected either before or after the body depending on whether the template has a slot.

Slots can be used to inform how a template merges into another template.

If you pass a Style into a template (a built in Interface), the template will change its behavior and styling based on the Style passed in.
When interfaces are implemented in the language, you will be able to implement your own Styles, by creating types that implement the Style interface. 

#### The body section
is used to define the content of the template. This is a string.

Inside the body of a template, you can nest more templates using square brackets. 


## Template example

[basic.Code:

    [head_of_element: content of element that renders as text (markdown / metadata / alt tag / code ..etc)]

    [ loop text in text_array: 
        [text]
    ]

    [if not text.isBlank():
        [ text: plus some extra text in the same element as the variable text ]
    ]

    [ another_template_insert ]
]

### Errors
If an error is added to a template at runtime, it will be displayed in the console, rather than injected into the template. 
This usually means that the variable with the error has reverted to its default value, which will be shown.

[basic.Red, basic.Size(0.85): *This behaviour is an uncertain part of the design, tuples that contain errors might be better completely hidden from the template rather than a zero value being shown.*]


[docs_styles.Footer]