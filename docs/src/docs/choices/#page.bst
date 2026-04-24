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
    The Alpha compiler supports unit variants.
    Payload variants, tagged variant bodies, default values, and constructor calls are deferred.

    You create and access choices using a double colon.

    [codeblock, $code("bst"):
        #Status :: Ready, Busy, Offline;

        current Status = Status::Busy

        pass_status |status Status| -> Status:
            return status
        ;

        selected = pass_status(current)
    ]

    Choices work with assignment and pattern matching.

    [codeblock, $code("bst"):
        #Status :: Ready, Busy;

        current ~= Status::Ready
        current = Status::Busy

        label ~= "unset"
        if current is:
            case Ready => label = "ready"
            case Busy => label = "busy"
        ;
    ]

    These richer choice forms are reserved for post-Alpha design work and currently reject with structured diagnostics:

    [codeblock, $code("bst"):
        -- Payload variant: deferred.
        Response :: Error String, Success;

        -- Tagged variant body: deferred.
        Response :: Pending | retry_count Int |, Success;

        -- Constructor call: deferred.
        value = Response::Success()
    ]

]
