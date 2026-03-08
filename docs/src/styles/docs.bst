import @libs/html/doc
-- Some basic testing styles for the documentation website

# page = [:
    [doc.prelude, doc.en]
    
    [doc.head:
        <meta charset="UTF-8">
        <meta name="viewport"content="width=device-width, initial-scale=1">
        <link rel="icon"href="[$slot("favicon")]">

        [doc.title: "Beanstalk Documentation"]
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

# title = [: <h1 style="font-size: 2em;">[$slot]</h1>]

Palette = |
    -- Colors for the documentation website
    red String = [:<span style="color: hsl(0, 73%, 55%);">[$slot]</span>],
    green String = [:<span style="color: green;">[$slot]</span>],
    blue String = [:<span style="color: blue;">[$slot]</span>],
|

# palette = Palette()

