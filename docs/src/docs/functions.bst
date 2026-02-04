@(libs/html/Basic)
@(styles/Docs)
@(./components)
[Basic.page:

[Docs.Navbar]

[Docs.header, Basic.center: [Basic.title: FUNCTIONS]]

Functions in Beanstalk pass arguments by reference.

```beanstalk
    -- The simplest possible function
    -- The name of the function first,
    -- Followed by a parameter definition that defines what arguments must be passed in
    main ||:
        -- Some code goes in here!
    ;
```

## The function signature
The arrow symbol is used to define the return signature of a function. 
Functions can return multiple values. 
If the function returns values, 
it must use a type signature or the name of one of the arguments if it's returning that reference back to the caller.

```beanstalk

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
    Error = |
        msg String,
        context ~{String},
    |

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

        io([: Did not error!])
    ;

    panics ||:
        canError("") e!:
            io("Error: ", e)
            #panic "Panicking due to error"
        ;
    ;
```

## Closures
Closures in Beanstalk are nameless functions used as values.
They use the same syntax as functions, but do not have a name and therefore cannot be declared at the top level.

Closures may only appear inside a function or method body and are always expressions (rvalues).

```beanstalk
    update_total |values {Int}| -> Int:
        total ~= 0

        apply = |value Int|:
            total += value
        ;

        loop v in values:
            apply(v)
        ;

        return total
    ;
```

### Capturing Variables
A closure may reference values from its surrounding scope.
This is called capture.

Captures are implicit, references by default and subject to the same mutability and exclusivity rules as normal variables.

### Returnning closures from functions
Functions may return closures as values.

To represent a closure type, Beanstalk uses the `Fn` keyword followed by the parameter and return type signatures inside of brackets.

```beanstalk
    -- Defining the type of a closure that takes an Int and returns a String
    Fn(Int -> String)
```

```beanstalk
    map |items {Int}, f Fn(Int -> Int)| -> {Int}:
        result ~= {}
        loop item in items:
            result.push(f(item))
        ;
        return result
    ;

    each |items {Int}, f Fn(Int)|:
        loop item in items:
            f(item)
        ;
    ;
```



[Docs.Footer]