# Errors and Panics

Errors are treated as values in Beanstalk, and
they represent Result types similar to Rust.

Any function that can return an error must have its error handled.

The bang symbol `!` marks one error return slot and handles error-returning calls.

```beanstalk
    parse_number |text String| -> Int, Error!:
        if text.is_empty():
            return! Error("Parse", "int.empty", "Missing number")
        ;

        return 42
    ;

    -- Fallback value.
    value = parse_number("") ! 0

    -- Bubble the error to the surrounding function.
    wrapper |text String| -> Int, Error!:
        value = parse_number(text)!
        return value
    ;

    -- Named handler with a fallback value.
    recover |text String| -> Int:
        value = parse_number(text) err! 0:
            io(err.message)
        ;

        return value
    ;

    -- By default, collection .get(index) returns a Result<Elem, Error>.
    -- If the index is out of bounds, the error value is an Error with a message field.
    my_list ~= {1, 2, 3}

    -- Handle the out-of-bounds case with a fallback and named error scope.
    fallback = my_list.length() ! 0
    value = my_list.get(5) err! fallback:
        io("Index out of bounds error: ", err.message)
    ;

    -- All collection methods enforce strict runtime validation.
    -- push, remove, and length also validate inputs and produce structured errors
    -- on invalid receivers or out-of-bounds indices. The backend handles
    -- propagation automatically for these methods.
```

## Using the ? operator
```beanstalk

    -- Using the Option type (?) we can represent that a value might not exist
    -- This function returns a string or none.
    find_url |has_url Bool| -> String?:
        if has_url:
            return "https://nyejames.com"
        ;

        return none
    ;

    url String? = find_url(false)
```

## Panics
There are cases where you want to either catch unexpected state for debugging, 
or prevent the program from continuing if a certain condition is not met or an error is thrown.

For these cases the panic is used to mark functions that can panic explicitly at runtime.

Functions that can panic at runtime should only be used in cases where the program should absolutely not continue if the function fails.
Such as if there is a high risk of undefined behaviour in a critical and complex part of a program.
Effectively creating a runtime assertion.

```beanstalk
    #panic "Message about the panic"
```
