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
    - **Range loops can bind the current counter**
    - **Collection loops can bind the current item**
    - **An optional second binding provides the zero-based iteration index**
    - **Loop bindings are written after the loop condition**
    - **When bindings exist, they always use [$code("bst"):|...|] syntax**
    - **No [$code("bst"):reverse] keyword**; direction is inferred from the bounds
    - **[$code("bst"):by]** controls step size and works with both directions

    Loops come in two forms:

    1. **Conditional loops** (repeat while a condition is true)
    2. **Iteration loops** (iterate over a collection or a numeric range)

    ### Conditional loops

    A conditional loop repeats for as long as its condition stays true.

    [codeblock, $code("bst"):
        loop is_connected():
            io("still connected")
        ;
    ]

    Conditional loops usually do not need bindings.

    ### Iteration loops

    Iteration loops evaluate the loop header as an iterable source and can optionally bind values for each iteration.

    Iteration can step through either:

    - a **collection** (yielding the current item), or
    - a **range** (yielding the current counter)

    Bindings are written after the loop condition using `|...|`.

    [codeblock, $code("bst"):
        loop items |item|:
            io(item.to_string())
        ;
    ]

    A second binding can be added to receive the current zero-based index:

    [codeblock, $code("bst"):
        loop items |item, index|:
            io([index]: [item])
        ;
    ]

    The form is determined entirely by the loop header:

    - If the header contains **`to`** or **`upto`**, it is a **range loop**.
    - Otherwise, if the header ends with loop bindings, it is a **collection loop**.
    - Otherwise, it is treated as a **conditional loop**.

    ### Range loops

    - `to` uses an **exclusive** end bound
    - `upto` uses an **inclusive** end bound

    [codeblock, $code("bst"):
        loop 0 to 10 |i|:
            io(i.to_string())
        ;
        -- yields: 0, 1, 2, 3, 4, 5, 6, 7, 8, 9

        loop 0 upto 10 |i|:
            io(i.to_string())
        ;
        -- yields: 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10
    ]

    Range loops can also bind a zero-based iteration index:

    [codeblock, $code("bst"):
        loop 10 to 15 |value, index|:
            io([index]: [value])
        ;
        -- yields: 0: 10
        -- yields: 1: 11
        -- yields: 2: 12
        -- yields: 3: 13
        -- yields: 4: 14
    ]

    You can specify a step using `by`.

    [codeblock, $code("bst"):
        loop 0 to 10 by 2 |i|:
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
        loop 10 to 0 |i|:
            io(i.to_string())
        ;
        -- yields: 10, 9, 8, 7, 6, 5, 4, 3, 2, 1
    ]

    You can also supply an explicit step:

    [codeblock, $code("bst"):
        loop 10 upto 0 by 2 |i|:
            io(i.to_string())
        ;
        -- yields: 10, 8, 6, 4, 2, 0
    ]

    - When bounds imply descending iteration, `by` is treated as a magnitude and the compiler applies the correct sign automatically.
    - A step of `0` is invalid.

    Float ranges are supported, but **`by` should be considered required** to prevent ambiguous or non-terminating loops.

    [codeblock, $code("bst"):
        loop 0.0 to 1.0 by 0.1 |t|:
            io(t.to_string())
        ;
    ]
]
