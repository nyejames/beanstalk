import @html {center}
import @styles/docs {navbar, title, section, codeblock, codesnippet, theme_head}

page_title #= "Core Math"
page_description #= "Math constants and Float helper functions."
page_head #= theme_head

#[navbar]
#[title, center: CORE MATH]

#[section, $markdown:
    [codesnippet:@core/math] provides common [codesnippet:Float] constants and helper functions.

    Constants include [codesnippet:PI], [codesnippet:TAU], and [codesnippet:E].
    Functions include [codesnippet:sin], [codesnippet:cos], [codesnippet:tan], [codesnippet:atan2], [codesnippet:log], [codesnippet:pow], [codesnippet:sqrt], [codesnippet:abs], [codesnippet:min], [codesnippet:max], and [codesnippet:clamp].

    [codeblock, $code("bst"):
        import @core/math {PI, sin, clamp}

        wave = sin(PI / 2.0)
        bounded = clamp(wave, 0.0, 1.0)

        io([:bounded=[bounded]])
    ]

    On the HTML JavaScript path these helpers lower to JavaScript [codesnippet:Math] expressions.
]
