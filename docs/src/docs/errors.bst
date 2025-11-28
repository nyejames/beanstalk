#import("libs/html/Basic")
#import("styles/Docs_styles")
#import("./Components")
[Basic.page:
[Components.Navbar]

[Docs_styles.Header Basic.center: [Basic.title(1): ERROR HANDLING AND OPTIONALS]]
[Basic.Page:

Errors are treated as values in Beanstalk, and
they represent Result types similar to Rust.

Any function that can return an error must have its error handled.

The bang symbol ! is used for creating Result types and handling errors.

```beanstalk
    func_call_that_can_return_an_error() !:
        -- Error handling code
    ;

    -- Here, we define a type called 'BadStuff' that we will use as our error value.
    BadStuff:
        msg String
    ;

    -- This function can return a String and an Int or a BadStuff error
    -- The ! indicates that instead of the normal return values, the error value could be returned instead of the other two
    -- Using return! returns only the error value
    -- The regular return doesn't return the error value
    -- Only one value can use the ! symbol to represent the error value
    parent_func || -> String, Int, BadStuff!:
        text = func_call_that_can_return_an_error() !err:
            print("Error: ", err)
            return! BadStuff(err.msg)
        ;

        return text, 42
    ;
    
    text, number = parent_func() !err:
        print("Error from parent_func: ", err.msg)
        return
    ;

    -- Handling an error with default values in case of an error
    string_returned, number_returned = parent_func() !("", 0)

    -- Bubbling up errors without handling them
    another_parent_func || -> String, Int, BadStuff!:
        -- Since this function has the same return signature, 
        -- it can be directly returned without handling the error here
        -- The ! represents that the value is a Result type
        return parent_func()!
    ;

    -- By default, accessing items in a collection by their index using .get() returns an error if the index is out of bounds
    -- Unlike other languages where this can cause a panic or exception by default

    my_list ~= [1, 2, 3]

    -- If you wanted to both open a scope for handling the error and provide a default value, you could do it like this:
    -- Default to the last index, and also print an error message
    value ~= my_list.get(5) !(
        my_list.length() - 1
    ) !msg:
        print("Index out of bounds error: ", msg)
    ;
```

## Using the ? operator
```beanstalk

    -- Using the Option type (?) we can represent that a value might not exist
    -- This function returns a string or None
    getURL || -> String?:
        response = getNetworkRequest()

        if response is None:
            return None

        return response.body
    ;

    -- We can use some ? syntax sugar to set a default value if the value is None
    url = getURL() ?("https://nyejames.com")

    -- This function always returns a Response, as we've handled the None case inside the function
    getURL || -> Response:
        return getNetworkRequest() ?(
            Response("Default Body")
        )
    ;

    -- Ignoring the error syntax sugar and returning errors with the values
    -- Notice, not using the ! 
    -- Instead, we will use an option '?' to represent that the error could be None
    -- This is equivalent to the pattern used for error handling in Go
    parent_func_no_sugar || -> String, Int, BadStuff?:
        text, number = func_call_that_can_return_an_error() !:
            return "", 0, BadStuff("An error occurred")
        ;

        return text, number, None
    ;

    -- We are just treating the error as an optional here instead
    -- So we can seperately check if there was an error
    text, number, error? = parent_func_no_sugar()

    if error is not None:
        print("Error from parent_func_no_sugar: ", error.msg)
        return
```

## Panics
There are cases where you want to either catch unexpected state for debugging, 
or prevent the program from continuing if a certain condition is not met or an error is thrown.

For these cases the panic is used to mark functions that can panic explicitly at runtime.

Functions that can panic at runtime should only be used in cases where the program should absolutely not continue if the function fails.
Such as if there is a high risk of undefined behaviour in a critical and complex part of a program.
Effectively creating a runtime assertion.

```beanstalk
    #panic("Message about the panic")
```

]

[Footer]