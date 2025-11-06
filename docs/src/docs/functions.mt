[#import(@libs/html/basic)]
[#import(@styles/docs_styles)]
[#import(@./components)]

[docs_styles.Navbar]

[docs_styles.Header, basic.Center: [basic.Title: FUNCTIONS]]

Functions in Beanstalk pass arguments by reference.

[basic.Code:
    -- The simplest possible function
    -- The name of the function first,
    -- Followed by a parameter definition that defines what arguments must be passed in
    main ||:
        -- Some code goes in here!
    ;
]

## The function signature
The arrow symbol is used to define the return signature of a function. 
Functions can return multiple values. 
If the function returns values, 
it must use a type signature or the name of one of the arguments if it's returning that reference back to the caller.

[basic.Code:

    get_doubled |value Int| -> Int:
        -- Creates a new immutable integer using value
        return value * 2
    ;

    -- This function returns a reference to the x value passed in
    -- The only names you can use in the return type are the names of arguments that are passsed in.
    -- This is to help the compiler understand lifetimes
    multipleReturns |x ~Int| -> x, Bool:
        x += 1
        return x, x is > 0
    ;

    -- Calling a function
    value, is_positive = multipleReturns(5)

    -- Creating an error type
    Error:
        msg String,
        context ~{String},
    ;

    -- Functions use the '!' to show that they can return an error instead of any other values
    -- This error must always be handled at the call site,
    -- The '!' symbol provides convenient syntax sugar for this
    canError |x String| -> String, ~Error!:
    
        if x.is_empty():
           return Error(msg = "WHoops", context = {})
        ;

        return x + " is cool"
    ;

    defaultIfError || -> String:
        return canError("Sam") ! ""
    ;

    bubble_up_error_with_context |name String| -> ~Error!:
        canError(name) ~err!:
            err.context.push("bubbled up from this function")
            return err
        ;

        [: Did not error!]
    ;

    panics ||:
        canError("") e!:
            print("Error: ", e)
            #panic()
        ;
    ;
]

[docs_styles.Footer]