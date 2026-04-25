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
    Alpha supports literal patterns, relational ordered-scalar patterns, guards, unit choice variants, and explicit `else =>` fallback arms.

    Non-choice matches must include `else =>`.
    Choice matches can omit `else =>` only when every unit variant is covered and no arm has a guard.

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

    Choices use unit variants in the Alpha surface.
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

    Negated patterns and capture/tagged patterns such as `case Item |value| =>` are deferred for Alpha.
    Beanstalk does not use wildcard `case _ =>` arms; use `else =>` for fallback behavior.
]
