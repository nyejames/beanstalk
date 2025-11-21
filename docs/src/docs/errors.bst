#import(@libs/html/basic)
#import(@styles/docs_styles)
#import(@./components)
[basic.page:
[Navbar]

[Header center: [title(1): ERROR HANDLING AND OPTIONALS]]
[Page:

Errors are treated as values in Beanstalk, 
any function that can return an error must have its error handled.

The bang symbol ! is used for handling errors and specifying which type is returned as the possible error.

Any type that uses a ! can use the same syntax to bubble up or handle errors.



[#Code:
    func_call_that_can_return_an_error() !:
        -- Error handling code
    ;

    -- Here, we define a type called 'Result' that we will use as our error value.
    Result:
        msg String
    ;

    -- This function can return a String
    -- But it can optionally return an error instead.
    parent_func || -> String, Result!:
        return func_call_that_can_return_an_error() !err:
            print("Error: ", err)
            return "", err
        ;
    ;

    -- Handling an error with a default value 
    string_returned = parent_func() !("default value")
]

## Using the ? operator
[#Code:
    getURL || -> String:
        return getNetworkRequest() ?("")
    ;

    -- Returns a string or None
    getURL || -> String?:
        return getNetworkRequest()
    ;
]

## Asserts and Panics
There are cases where you want to either catch unexpected state for debugging, 
or prevent the program from continuing if a certain condition is not met or an error is thrown.

For these cases the panic is used to mark functions that can panic explicitly at runtime.
You can't use the panic keyword unless the function has a panic as one if it's possible return values.

You can also create asserts that will stop the program if a condition is not met. 
These are removed by the compiler in release builds.

Functions that can panic at runtime should only be used in cases where the program should absolutely not continue if the function fails.
Such as if there is a high risk of undefined behaviour in a critical and complex part of a program.
Effectively creating a runtime assertion.

[#Code:
    -- stops the program if 1 is not equal to 1
    -- Runs the code in the block first
    #assert(1 is not 1, "1 is not 1 .. uh oh")
    #panic()
]

[Footer]