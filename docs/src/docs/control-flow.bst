@(libs/html/Basic)
@(styles/Docs)
@(./components)
[Basic.page:
[Docs.navbar]

[Docs.header Basic.center: [Docs.title(1): CONTROL FLOW]]

[Basic.Page:
## If Statements
If statements can be used as expressions or statements.
They can also be used for pattern matching, all with a similar syntax.

```beanstalk
    v = 1

    if v is not 1:
        io("nope")
    else
        io("yep")
    ;

    io(msg) -- prints "defintely nope"
```

# Loops
Only 1 keyword for loops "loop". 

Using the "in" keyword, you can specify an integer, float or collection to iterate through or define a new integer, float or collection. 

```beanstalk
    loop thing in things:
        io(thing)
    ;

    loop -20 to big_number:
        io("hello")
    ;

    -- reverse loop
    loop n in big_number to smaller_number:
        io(n)
    ;

    loop item in collection:
        io(item.to_string())
    ;

    -- Getting the index
    loop item, index in collection:
        io(index.to_string())
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
```

# Pattern matching
Pattern matching is done by extending an if statement with a new keyword "case".

This can be used in between the first if condition and an optional else keyword.

```beanstalk
    -- Pattern matching
    thing = 69

    if thing is 0:
        io("zero")
    case < 0
        io("negative")
    case > 1
        io("big number")
    case not 5
        io("not 5")
    else
        io("only one")
    ;
    
    -- Matching choices
    if Catagory is A:
        io("Category A")
    case B
        io("Category B")
    case C |message| 
        io([:Category C with message: [message]])
    ;

    loop c in collection_of_choices:
        if c is Thing::cool:
            io(..)
        else
            io(..)
        ;
    ;
```

[Basic.footer]