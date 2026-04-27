import @lib/html {format, style}
import @styles/docs {navbar, title, section, codeblock, theme_head}

# page_title = "Choices"
# page_description = "Choices in Beanstalk."
# page_head = theme_head

#[navbar]
#[title, style.center: CHOICES]

#[section, $markdown:

    # Choices (enums)
    Choices are a way to define a set of possible values.
    The Alpha compiler supports unit variants and record-body payload variants.

    You create and access choices using a double colon.

    [codeblock, $code("bst"):
        # Status :: Ready, Busy, Offline;

        current Status = Status::Busy

        pass_status |status Status| -> Status:
            return status
        ;

        selected = pass_status(current)
    ]

    Choices work with assignment and pattern matching.

    [codeblock, $code("bst"):
        # Status :: Ready, Busy;

        current ~= Status::Ready
        current = Status::Busy

        label ~= "unset"
        if current is:
            case Ready => label = "ready"
            case Busy => label = "busy"
        ;
    ]

    ## Payload variants
    Payload variants must use record-body syntax.

    [codeblock, $code("bst"):
        Response ::
            Success,
            Err |
                message String,
            |,
            Pending |
                retry_count Int,
                message String,
            |,
        ;

        -- Unit variant construction
        ok = Response::Success

        -- Payload variant construction
        error = Response::Err("bad")
        pending = Response::Pending(3, message = "waiting")
    ]

    Payload fields are accessed only through pattern matching.

    [codeblock, $code("bst"):
        if response is:
            case Success => io("done")
            case Err(message) => io(message)
            case Pending(retry_count, message) => io(message)
        ;
    ]

    ## Invalid syntax
    Unit variants are values, not constructor calls.

    [codeblock, $code("bst"):
        -- Invalid: empty constructor call on a unit variant
        bad = Response::Success()
    ]

    Payload shorthand is not supported.

    [codeblock, $code("bst"):
        -- Invalid: shorthand payload declaration
        Response :: Err String, Success;
    ]

    Capture rename syntax is deferred.

    [codeblock, $code("bst"):
        -- Invalid: capture rename syntax is not supported yet
        case Err(text as message) => io(text)
    ]

    ## Deferred features
    - Direct payload field access (e.g. `response.message`).
    - Generic choice declarations.
    - Recursive choice declarations.
    - Payload structural equality.
    - Capture rename syntax in patterns.
    - Nested payload patterns and nested exhaustiveness.

]
