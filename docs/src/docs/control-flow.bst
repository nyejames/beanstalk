[Navbar]

[Header center: [title(1): CONTROL FLOW]]

[Page:
## If Statements
If statements can be used as expressions or statements.
They can also be used for pattern matching, all with a similar syntax.

[#Code:
    v = 1

    if v is not 1:
        print("nope")
    else
        print("yep")
    ;

    -- Using if like a match expression
    -- Parenthesis are used instead of colons for if expressions
    x = 5
    msg = if x is 0 (
        "nope"
    ) else (
        "defintely nope"
    )

    print(msg) -- prints "defintely nope"
]

# Loops
Only 1 keyword for loops "loop". 

Using the "in" keyword, you can specify an integer, float or collection to iterate through or define a new integer, float or collection. 

[#Code:
    loop thing in things:
        print(thing)
    ;

    loop -20 to big_number:
        print("hello")
    ;

    -- reverse loop
    loop n in big_number to smaller_number:
        print(n)
    ;

    loop item in collection:
        print(item.to_string())
    ;

    -- Getting the index
    loop item, index in collection:
        print(index.to_string())
    ;

    -- Using a loop in a scene
    names = {"Priya", "Sam", "Chad"}
    [main:
        [loop name in names:
            Name: [name]
        ]
    ]

    -- scene looks like:
    -- Name: Priya
    -- Name: Sam
    -- Name: Chad
]

# Pattern matching
[#Code:
    -- Pattern matching
    -- Must be exhaustive
    thing = 69
    if thing is:
        0: print("zero")
        < 0: print("negative")
        > 1: print("big number")
        not 5: print("not 5")
        else: print("only one")
    ;

    -- Pattern matching as an expression
    name = "Bob"
    msg = if name is (
        "Sam": 
            "yo, whats up"
        ;
        "Priya": "hey!"
        else: "go away"
    )

    loop c in collection_of_choices:
        if c is Thing::cool: 
            print(..)
        else
            print(..)
        ;
    ;

]

[Footer]