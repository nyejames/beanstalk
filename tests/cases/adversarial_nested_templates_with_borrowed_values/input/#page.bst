name = "Ada"
score ~= 42

header = [:
    [name]
]

body = [:
    [header]
    current score [score]
]

score = score + 1
footer = [:
    done [name] [score]
]

io(body)
io(footer)
