#import(@libs/html/basic)
#import(@styles/docs_styles)
#import(@./components)
[basic.page:

[docs_styles.Navbar]

[docs_styles.Header, basic.Center: [basic.title(1): MARKTHROUGH FILES]]

[basic.Section:

# Markthrough
Markthrough files are an extension of the Beanstalk template syntax into its own file type.
Markthrough files are turned into HTML by Beanstalk's build system and always use the Beanstalk markdown parser by default.

Markthrough files use the '.mt' extension and form the foundation of Beanstalk HTML projects.

In an HTML project, each markthrough file can act as an entry point for a web page or a component.

[basic.Code:
    [#import(@html/basic)]
    [#import(@some_module/CONSTANT)]
    [basic.Section:

    The open square bracket is used to start a new template. 
    The colon begins the template body. 
    This is defining the main template, 
    which by default will add a main tag to the HTML and wrap the content in a centered container.

    PAGE is a top-level style that unlocks all the basic HTML styles for nested templates.
    It uses the built-in markdown parser function to parse this template body into markdown at compile time.

    # A Heading
    Some text that has a [@example.com: link to example.com] inside it.

    A constant variable from some imported code inserted into the template: [ CONSTANT ].

    The body of a template is mostly like a simple flavour of markdown, 
    the main difference being square brackets [basic.Red: *cannot*] be used without an escape character or by using backticks to create a raw string.

    ]
]

## Imports
Markthrough files can import other HTML, Markthrough, CSS, Wasm, JS or Beanstalk files.
This gives them access to custom styles, components or runtime scripts.

The syntax for this is:
[basic.Code:
    [#import(@html/basic)]
]

By creating a new child template and defining an import inside the template head, 
the whole file gets access to everything that is exported from that file.

In this example, 
we are importing a basic HTML styles file that will give us access to styles we can use in the head of templates.

We can then use those styles anywhere in our file, namespaced to the name of the file. 
Styles use PascalCase by convention.

You can also use functions that return styles inside of the template head.

[basic.Code:
    [#import(@html/basic)]

    Here is a normal paragraph. 
    [basic.Red: And now this is some red text inside this paragraph.]

    Here is an example of using a function that returns a style.
    This function runs at compiletime and will return the correct table style depending on the argument you give it.
    [basic.table(3):
        [: Column 1] [: Column 2] [: Column 3]
    ]
]


# HTML Basic library

## Scene Style Keywords
More styles and template configs will be added overtime. Here are some of the ideas so far:

### Style
[basic.table(2):
    [: Property] [: Description]

    [: bg] [: Changes the background color of the element (will eventually just be passed a style argument such as a color and will automatically make those style changes to the background)]
    [: rgb, hsl, hex] [: The color of the element. Color keywords will also be supported in the future]
    [: code] [: A code block]
    [: blank] [: removes all styling from the element]
    [: title] [: An even bigger, more styalized heading]
]

### Layout
[basic.table(2):
    [: Property] [: Description]

    [: pad] [: The padding around the element]
    [: space] [: The margin around the element]
    [: center] [: Centers the element horizontally. Pass in a bool of true to center vertically]
    [: table] [: A table element]
    [: size] [: The size of the text in the element]
    [: nav] [: A navigation element. Basically a nav bar, will automatically add ul tags for child templates]
    [: order] [: The order of the element inside a container]
    [: gap] [: Creates a gap between elements]

    [: main] [: The main element of the page. Creates a main tag with a centered container]
    [: header] [: The header element of the page. Creates a header tag with a centered container]
    [: footer] [: The footer element of the page. Creates a footer tag with a centered container]
    [: section] [: A section element of the page. Creates a section tag with a centered container]
]

### Interactive 
[basic.table(2):
    [: Property] [: Description]

    [: button] [: A button]
    [: input] [: An input element]
    [: click] [: You can provide a function as an argument to the click event]
    [: form] [: A form element]
    [: option] [: An option element]
    [: dropdown] [: A dropdown element]
]

### Media
[basic.table(2):
    [: Property] [: Description]

    [: img] [: An image element ]
    [: video] [: A video element ]
    [: audio] [: An audio element ]
    [: alt] [: An alternative text for media ]
]

### HTTP 
These are directly from the HTMX library and will be the same keywords, but without the leading 'hx-' prefix. More to be added soon!
[basic.table(2):
    [: Property] [: Description]

    [: get] [: ]
    [: post]  [: ]
    [: swap] [: ]
]

### Other
[basic.table(2): 
    [: Property] [: Description]

    [: ignore] [: Creates a comment, but does not add it to the final output]
    [: canvas] [: An HTML canvas element ]
]

## Unique Scene Logic
Variables and expressions are typecast into strings when put into a template. This is the behaviour of all the numerical types and most of the string types. There are some exceptions to how other types are handled by templates.

### Booleans
If statements in a templatehead will conditonally hide or show itself based on the value of the boolean. 
If the boolean is true, the template and it's children will be shown, if false, it will be hidden.

[basic.Code:
    logged_in = false

    -- Scene is hidden
    [if not logged_in:
        [button: Log In]
    ]
]


]

[docs_styles.Footer]
