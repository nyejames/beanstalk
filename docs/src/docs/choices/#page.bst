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
    They are similar to enums in Rust (tagged unions).

    You create and access choices using a double colon.

    [codeblock, $code("bst"):
        Choice :: Option1, Option2, Option3;

        option = Choice::Option2

        -- Another example with data attached to the Error choice and a Plex attached to the Pending choice
        ResponseKind ::
            Success,
            Error String,
            Pending |
                RetryCount Int,
                Timeout Float
            |,
        ;

        response = ResponseKind::Error("Network failure")
    ]

    Choices can also be defined with default values, the same way structs can.

    [codeblock, $code("bst"):
        Status ::
            Active Bool = true,
            Inactive Bool = false,
        ;

        -- No need to set a value, it defaults to 'true'
        currentStatus = Status::Active
    ]

]
