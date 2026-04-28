import @lib/html {format, style}
import @styles/docs {navbar, title, section, palette, table, codeblock, theme_head}

# page_title = "Pattern Matching"
# page_description = "Pattern matching in Beanstalk."
# page_head = theme_head

#[navbar]
#[title, style.center: PATTERN MATCHING]

#[section, $markdown:

    # Pattern matching
    Pattern matching is a control-flow tool for checking one value against multiple patterns.
    Alpha supports literal patterns, relational ordered-scalar patterns, guards, choice variant patterns (including payload captures), and explicit `else =>` fallback arms.

    Non-choice matches must include `else =>`.
    Choice matches can omit `else =>` only when every variant is covered and no arm has a guard.

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
        #Status :: Ready, Busy, Offline;

        current Status = Status::Busy
        label ~= "unknown"

        if current is:
            case Ready => label = "ready"
            case Busy => label = "busy"
            case Offline => label = "offline"
        ;
    ]

    Payload variants use constructor-like patterns with the original field names.
    Captured bindings are available in the arm body and in guards.
    Captures may be renamed with `as` when the arm-local name should differ from the payload field name.

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

    Capture originals must exactly match the declared payload field names.
    For example, `case Err(message as error_text)` binds the payload field `message` to the local name `error_text`.

    Exhaustiveness is tag-level: `case Err(message)` covers all `Err` values regardless of payload content.

    Negated patterns and general capture/tagged patterns such as `case Item |value| =>` are deferred for Alpha.
    Beanstalk does not use wildcard `case _ =>` arms; use `else =>` for fallback behavior.
]
