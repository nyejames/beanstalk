import @html {center}
import @styles/docs {navbar, title, section, palette, styled_table, codeblock, theme_head}

# page_title = "Functions"
# page_description = "Function declarations, returns, calls, and Alpha-scope result handling in Beanstalk."
# page_head = theme_head

#[navbar]
#[title, center: FUNCTIONS]

#[section, $markdown:
    [codeblock, $code("bst"):
        -- The simplest possible function
        main ||:
            io("Hello from a function")
        ;
    ]

    ## The function signature
    Function names come first, followed by parameters inside vertical pipes.
    The arrow symbol defines return slots.
    Functions can return multiple values.

    [codeblock, $code("bst"):

        get_doubled |value Int| -> Int:
            return value * 2
        ;

        pair || -> String, Int:
            return "Ana", 2
        ;

        doubled = get_doubled(21)
        name, count = pair()
    ]

    ## Arguments
    Positional arguments come before named arguments.
    Mutable access to an existing place must be explicit at the call site.

    [codeblock, $code("bst"):
        update |value ~Int|:
            value += 1
        ;

        value ~= 1
        update(~value)

        -- Fresh values can satisfy mutable parameters without '~'.
        update(1)
    ]

    ## Error-returning functions
    A function can mark one return slot with `!`.
    Error-returning calls must be handled with propagation, fallback values, or a named handler.

    [codeblock, $code("bst"):
        parse_number |text String| -> Int, Error!:

            if text.is_empty():
                return! Error("Parse", "int.empty", "Missing number")
            ;

            return 42
        ;

        with_fallback || -> Int:
            value = parse_number("") ! 0
            return value
        ;

        bubble_up |text String| -> Int, Error!:
            value = parse_number(text)!
            return value
        ;

        recover |text String| -> Int:
            value = parse_number(text) err! 0:
                io(err.message)
            ;

            return value
        ;
    ]

    ## Closures
    Closures and `Fn(...)` function-value types are deferred for Alpha.
    Do not use closure examples as current compiler support yet.
]
