import @libs/html/Basic
import @styles/Docs
import @./components
[Basic.page:

[Docs.Navbar]

[Docs.Header, Basic.Center: [Basic.title(1): BEANSTALK MARKDOWN]]

[Basic.Section:

# Beanstalk for Frontend
Beanstalk can be used to generate HTML via its powerful template syntax. 

Templates by default are used to create strings, but can be customised to parse into any format. 

Beanstalk has a built-in Markdown formatter that will parse templates into HTML at compiler time. 

The build system can also be configured to use templates at the top level of files any way it wishes.
For web projects, the build system uses these templates to automatically represent a web page. 
The top level template automatically uses the Markdown formatter and that template is used to build the web page.

The build system for web projects can then separate out the Wasm, JS glue code and HTML/CSS into seperate or combined files
while the user only has to write a single Beanstalk file per page.

Beanstalk files can still import additional HTML, CSS, JS or Beanstalk files to help structure larger projects.

```beanstalk
    #import "html/Basic"
    #import "some_module/CONSTANT"

    -- This template is just a the top level of the file
    -- It's also not assigned to any variabels or functions
    -- This means the build system recognises it as a special top-level template
    [Basic.section:

    The open square bracket is used to start a new template. 
    The colon begins the template body. 
    This is defining the main template, 
    which by default will add a main tag to the HTML and wrap the content in a centered container.

    PAGE is a top-level style that unlocks all the basic HTML styles for nested templates.
    It uses the built-in markdown parser function to parse this template body into markdown at compile time.

    # A Heading
    Some text that has a [Basic.link("example.com"): link to example.com] inside it.

    A constant variable from some imported code inserted into the template: [ CONSTANT ].

    The body of a template is mostly like a simple flavour of markdown, 
    the main difference being square brackets [Basic.red: *cannot*] be used without an escape character or by using backticks to create a raw string.

    ]
```

# HTML Basic library

## Scene Style Keywords
More styles and template configs will be added overtime. Here are some of the ideas so far:

### Style
[Basic.table(2):
    [: Property] [: Description]

    [: bg] [: Changes the background color of the element (will eventually just be passed a style argument such as a color and will automatically make those style changes to the background)]
    [: rgb, hsl, hex] [: The color of the element. Color keywords will also be supported in the future]
    [: code] [: A code block]
    [: blank] [: removes all styling from the element]
    [: title] [: An even bigger, more styalized heading]
]

### Layout
[Basic.table(2):
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
[Basic.table(2):
    [: Property] [: Description]

    [: button] [: A button]
    [: input] [: An input element]
    [: click] [: You can provide a function as an argument to the click event]
    [: form] [: A form element]
    [: option] [: An option element]
    [: dropdown] [: A dropdown element]
]

### Media
[Basic.table(2):
    [: Property] [: Description]

    [: img] [: An image element ]
    [: video] [: A video element ]
    [: audio] [: An audio element ]
    [: alt] [: An alternative text for media ]
]

### HTTP 
These are directly from the HTMX library and will be the same keywords, but without the leading 'hx-' prefix. More to be added soon!
[Basic.table(2):
    [: Property] [: Description]

    [: get] [: ]
    [: post]  [: ]
    [: swap] [: ]
]

### Other
[Basic.table(2): 
    [: Property] [: Description]

    [: ignore] [: Creates a comment, but does not add it to the final output]
    [: canvas] [: An HTML canvas element ]
]

## Unique Scene Logic
Variables and expressions are typecast into strings when put into a template. This is the behaviour of all the numerical types and most of the string types. There are some exceptions to how other types are handled by templates.

### Booleans
If statements in a templatehead will conditonally hide or show itself based on the value of the boolean. 
If the boolean is true, the template and it's children will be shown, if false, it will be hidden.

```beanstalk
    logged_in = false

    -- Scene is hidden
    [if not logged_in:
        [button: Log In]
    ]
```


]

[Docs.Footer]
