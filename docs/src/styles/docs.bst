import @libs/html/doc
-- Some basic testing styles for the documentation website

# page = [:
    [doc.prelude, doc.en]
    
    [doc.head:
        [doc.title: "Beanstalk Documentation"]
        [doc.style:
            body {
                padding: 1rem;
            }
        ]
    ]

    [doc.body:
        [..]
    ]
]

# navbar = [:
    <nav></nav>
]

# footer = [:
    <div></div>
]

# title = [: <h1 style="font-size: 2em;">[..]</h1>]

Pallette = |
    -- Colors for the documentation website
    red String = [:<span style="color: red;">[..]</span>],
    green String = [:<span style="color: green;">[..]</span>],
    blue String = [:<span style="color: blue;">[..]</span>],
|

# pallette = Pallette()

