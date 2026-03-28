import @lib/html {format, style}
import @styles/docs {navbar, title, section, palette, table, data, codeblock, codesnippet, theme_head}

# page_title = "Language Basics"
# page_description = "Core language syntax and semantics in Beanstalk."
# page_head = theme_head

#[navbar]
#[title, style.center: CONTROL FLOW]

#[section, $markdown:
    ## If Statements

    [codeblock, $code("bst"):
        v = 1

        if v is not 1:
            io("nope")
        else
            io("yep")
        ;

        io(msg) -- prints "nope"
    ]
]

#[section, $markdown:
    # Loops
    ### Loops in Beanstalk

    Beanstalk uses a **single** loop keyword: [codesnippet: loop].

    - **[$code("bst"):to] / [$code("bst"):upto]** select range semantics (exclusive vs inclusive)
    - **Range loops yield the counter**
    - **Collection loops yield elements**
    - **No enumeration syntax yet**
    - **No [$code("bst"):reverse] keyword**; direction inferred from bounds
    - **[$code("bst"): by]** controls step size and works with both directions

    Loops come in two forms:

    1. **Conditional loops** (repeat while a condition is true)

    [codeblock, $code("bst"):
        loop is_connected():
            io("still connected")
        ;
    ]

    2. **Iteration loops** (iterate over a collection or a numeric range)

    Iteration loops bind a value each iteration and step through either:

    - a **collection** (yielding elements), or
    - a **range** (yielding a counter)

    [codeblock, $code("bst"):
        loop item in items:
            io(item.to_string())
        ;
    ]

    The form is determined entirely by the loop header:

    - If the header contains **`to`** or **`upto`**, it is a **range loop**.
    - Otherwise, it is treated as a **conditional loop**.

    - `to` for **exclusive** end bounds
    - `upto` for **inclusive** end bounds

    [codeblock, $code("bst"):
        loop i in 0 to 10:
            io(i.to_string())
        ;
        -- yields: 0, 1, 2, 3, 4, 5, 6, 7, 8, 9

        loop i in 0 upto 10:
            io(i.to_string())
        ;
        -- yields: 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10
    ]

    You can specify a step using `by`.

    [codeblock, $code("bst"):
        loop i in 0 to 10 by 2:
            io(i.to_string())
        ;
        -- yields: 0, 2, 4, 6, 8
    ]

    ### Direction is inferred from bounds

    Beanstalk automatically determines the iteration direction from the bounds:

    - If `start < end`, the default direction is ascending.
    - If `start > end`, the default direction is descending.

    With no `by`, the default step is `+1` for ascending ranges and `-1` for descending ranges.

    [codeblock, $code("bst"):
        loop i in 10 to 0:
            io(i.to_string())
        ;
        -- yields: 10, 9, 8, 7, 6, 5, 4, 3, 2, 1
    ]

    You can also supply an explicit step:

    [codeblock, $code("bst"):
        loop i in 10 upto 0 by 2:
            io(i.to_string())
        ;
        -- yields: 10, 8, 6, 4, 2, 0
    ]

    - When bounds imply descending iteration, `by` is treated as a magnitude (the compiler will apply the correct sign based on direction).
    - A step of `0` is invalid.

    Float ranges are supported, but **`by` should be considered required** to prevent ambiguous or non-terminating loops.

    [codeblock, $code("bst"):
        loop t in 0.0 to 1.0 by 0.1:
            io(t.to_string())
        ;
    ]
]

#[section, $markdown:

    # Pattern matching
    Pattern matching is done by extending an if statement with a new keyword "case".

    This can be used in between the first if condition and an optional else keyword.

    [codeblock, $code("bst"):
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
    ]
]