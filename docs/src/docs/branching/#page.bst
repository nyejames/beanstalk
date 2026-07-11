import @html {center}
import @styles/docs {navbar, title, section, palette, styled_table, codeblock, theme_head}

page_title #= "Branching"
page_description #= "If statements and pattern matching in Beanstalk."
page_head #= theme_head

#[navbar]
#[title, center: BRANCHING]

#[section, $md:

    # If  Statements
    [codeblock, $code("bst"):
        v = 1

        if v is not 1:
            io.line("nope")
        else
            io.line("yep")
        ;

        io.line(msg) -- prints "nope"
    ]

    If statements are non-exhaustive and there is no "if else" or "elif" in Beanstalk.

    Instead, use pattern matching for more than two branches.

    The whole if statement and optional "else" branch is a single block, 
    so there is only one semicolon at the end to close the statement.
]

#[section, $md:

    # Pattern  Matching
    Pattern matching is a control-flow tool for checking one value against multiple patterns.

    Non-choice matches must include ` else =>`.
    Choice matches can omit ` else =>` only when every variant is covered and no arm has a guard.

    [codeblock, $code("bst"):
        score = 84
        result ~= "unknown"

        if score is:
            < 0 => result = "invalid"
            >= 90 => result = "excellent"
            >= 70 if true => result = "passing"
            else => result = "retry"
        ;
    ]

    Match arms are separated by the next line-starting arm header or by the final semicolon that closes the whole match.
    There are no per-arm semicolons.

    [codeblock, $code("bst"):
        value = 1

        if value is:
            0 => io.line("zero")
            else =>
        ;
    ]

    This is the explicit way to say "match everything else and do nothing".

    Choices support unit and payload variant patterns.
    Exhaustive choice matches do not need a fallback arm when every variant is covered.

    [codeblock, $code("bst"):
        Status :: Ready, Busy, Offline;

        current Status = Status::Busy
        label ~= "unknown"

        if current is:
            Ready => label = "ready"
            Busy => label = "busy"
            Offline => label = "offline"
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
            Success => io.line("done")
            Err(message) => io.line(message)
            Pending(retry_count, message as pending_message) => io.line(pending_message)
        ;
    ]

    The names before ` as` must match the declared payload fields. The optional name after ` as` is the arm-local binding.

    Capture originals must exactly match the declared payload field names.
    For example, ` Err(message as error_text) =>` binds the payload field ` message` to the local name ` error_text`.

    Exhaustiveness is tag-level: ` Err(message) =>` covers all ` Err` values regardless of payload content.
    
    Fallback match arms use ` else =>`.
    The fallback body may be empty; an empty ` else =>` catches all remaining cases and does nothing.
    ` _ =>` and ` else => _` are not valid match syntax.
]
