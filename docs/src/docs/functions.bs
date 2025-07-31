[navbar]

[header center: [title(1): FUNCTIONS]]
[Page:

Beanstalk programs are comprised of functions inside of modules.

[code:

    -- The simplest possible function
    -- The name of the function first,
    -- Followed by a object definition that defines what arguments are passed in
    main ||:
        -- Some code goes in here!
    ;
]

## The function signature
The arrow symbol is used to define the signature of a function. 
Functions can return multiple values. If the function returns values, 
it must use a type signature or the name of one of the arguments if it's returning that reference back to the caller.

[code:

    get_doubled |value Int| -> Int:
        return value *= 2
    ;

    -- This function returns a reference to the x value passed in
    -- The only names you can use in the return type are the names of arguments that are passsed in.
    -- This is to help infer lifetimes
    multipleReturns |x Int| -> x, Bool:
        value = x + 1
        return (value, value is > 0)
    ;

    -- Calling a function
    value, is_positive = multipleReturns(5)

    canError |x String| -> String, Error!:
        return fetch(x)
    ;

    defaultIfError || -> String:
        return canError("example.com") ! ""
    ;

    panics ||:
        canError e!:
            print("Error: ", e)
            #panic 
        ;
    ;
]

]

[Footer]