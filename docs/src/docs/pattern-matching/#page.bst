import @html {center}
import @styles/docs {navbar, title, section, palette, styled_table, codeblock, theme_head, codesnippet}

# page_title = "Pattern Matching"
# page_description = "Pattern matching in Beanstalk."
# page_head = theme_head

#[navbar]
#[title, center: PATTERN MATCHING]

#[section, $markdown:

    # Pattern matching
    Pattern matching is a control-flow tool for checking one value against multiple patterns.

    Non-choice matches must include [codesnippet: else =>].
    Choice matches can omit [codesnippet: else =>] only when every variant is covered and no arm has a guard.

    [codeblock, $code("bst"):
        score = 84
        result ~= "unknown"

        if score is:
            case < 0 => result = "invalid"
            case >= 90 => result = "excellent"
            case >= 70 if true => result = "passing"
            else => result = "retry"
        ;
    ]

    Choices support unit and payload variant patterns.
    Exhaustive choice matches do not need a fallback arm when every variant is covered.

    [codeblock, $code("bst"):
        # Status :: Ready, Busy, Offline;

        current Status = Status::Busy
        label ~= "unknown"

        if current is:
            case Ready => label = "ready"
            case Busy => label = "busy"
            case Offline => label = "offline"
        ;
    ]

    Payload variants can be matched by writing the variant name followed by its payload field names in parentheses.

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

        if response is:
            case Success => io("done")
            case Err(message) => io(message)
            case Pending(retry_count, message as pending_message) => io(pending_message)
        ;
    ]

    The names before [codesnippet: as] must match the declared payload fields. The optional name after [codesnippet: as] is the arm-local binding.

    Capture originals must exactly match the declared payload field names.
    For example, [codesnippet: case Err(message as error_text)] binds the payload field [codesnippet: message] to the local name [codesnippet: error_text].

    Exhaustiveness is tag-level: [codesnippet: case Err(message)] covers all [codesnippet: Err] values regardless of payload content.
    
    Fallback match arms use [codesnippet: else =>], not a [codesnippet: case] pattern.
]
