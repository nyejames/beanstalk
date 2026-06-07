import @html {center}
import @styles/docs {navbar, title, section, codeblock, theme_head}

page_title #= "Core Math"
page_description #= "Math constants and Float helper functions."
page_head #= theme_head

#[navbar]
#[title, center: CORE MATH]

#[section, $markdown:
    `@core/math` provides common `Float` constants and helper functions.

    Constants include `PI`, `TAU`, and `E`.
    Functions include `sin`, `cos`, `tan`, `atan2`, `log`, `pow`, `sqrt`, `abs`, `min`, `max`, and `clamp`.

    [codeblock, $code("bst"):
        import @core/math {PI, sin, clamp}

        wave = sin(PI / 2.0)
        bounded = clamp(wave, 0.0, 1.0)

        io([:bounded=[bounded]])
    ]

    On the HTML JavaScript path these helpers lower to JavaScript `Math` expressions.
]
