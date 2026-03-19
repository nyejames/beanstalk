import @libs/html/{doc, format}
-- Some basic testing styles for the documentation website

# page = [:
    [doc.prelude, doc.en]
    
    [doc.head:
        <meta charset="UTF-8">
        <meta name="viewport"content="width=device-width, initial-scale=1">
        <link rel="icon"href="[$slot("favicon")]">

        [doc.title: Beanstalk Documentation]
        [doc.style:
            [$slot("css")]
        ]
    ]

    [doc.body:
        [$slot]
    ]
]

# navbar = [:
    <nav></nav>
]

# footer = [:
    <div></div>
]

# table = [format.table:
    [$insert("style"):border-collapse: collapse; border: 1px solid; padding: 1em;]
    [$slot]
]

# row = [:
    <tr style="border-bottom: 1px dotted grey;">[$reset, $children([:<td style="padding: 0.2em 0.5em;">[$slot]</td>]):[$slot]]</tr>
]

# header_row = [:
    <tr style="background-color: hsla(107, 100%, 36%, 0.23);">
        [$reset, $children([:
            <th style="border: 1px solid; padding: 0.5em; text-align: left;">[$slot]</th>
        ]):[$slot]]
    </tr>
]

# title = [: <h1 style="font-size: 2em;[$slot("style")]">[$slot]</h1>]

Palette = |
    -- Colors for the documentation website
    red String = [:<span style="color: hsl(0, 73%, 55%);">[$slot]</span>],
    green String = [:<span style="color: green;">[$slot]</span>],
    blue String = [:<span style="color: blue;">[$slot]</span>],
|

# palette = Palette()