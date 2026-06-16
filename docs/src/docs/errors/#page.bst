import @html {center}
import @styles/docs {navbar, title, section, styled_table, header, data, codeblock, theme_head}

page_title #= "Errors, Options, and Assertions in Beanstalk"
page_description #= "Error values, error propagation, option values, and always-checked assertions in Beanstalk."
page_head #= theme_head

#[navbar]
#[title, center: ERRORS, OPTIONS, AND ASSERTIONS IN BEANSTALK]

#[section, $markdown:
    This page tracks Beanstalk's current error-handling surface.
    The public `Error` value shape and `catch` forms shown here are current.

    Errors are values in Beanstalk.

    Functions that can fail return an `Error` value through a dedicated error return slot.

    That error slot is marked with `!`.

    Any call that can return an error must handle it.

    The core rule is:

    > If a function can return an error, the caller must either catch the error or bubble it upward.
]

#[section, $markdown:
    # Core syntax

    Error-returning functions mark one return slot with `!`.

    [codeblock, $code("bst"):
        parse_number |text String| -> Int, Error!:
            if text.is_empty():
                return! Error("Missing number", 200)
            ;

            return 42
        ;
    ]

    In this example:

    - `Int` is the success value
    - `Error!` is the error slot
    - `return!` returns through the error slot
    - normal `return` returns through the success path

    The `!` marker does not mean panic.

    It means this function has a typed error path that callers must handle.
    Expected failures should use `Error!` and `catch`.
    Assertions are for unrecoverable invariants and are the current explicit panic path.

    `catch:` and `catch |err|:` are the supported result recovery forms.
    `then` produces local values for a surrounding value-receiving site. Value-producing `if` and full match receivers reuse that mechanism now; catch recovery still uses it inside catch handlers until catch is refactored onto the shared value-block model.
    Public first-class `Result` values and raw fallible carrier storage remain deferred; use `!` return slots and call-site handling instead.
]

#[section, $markdown:
    # The Error type

    `Error` is a builtin language type.

    It carries a human-readable message and an integer code.

    Publicly, `Error` contains:

    [styled_table:
        [header: [: Field] [: Type] [: Meaning] ]

        [data:
            [: [$html:<code>message</code>] ]
            [: [$html:<code>String</code>] ]
            [: Human-readable explanation ]
        ]

        [data:
            [: [$html:<code>code</code>] ]
            [: [$html:<code>Int</code>] ]
            [: Stable numeric error code. Defaults to [$html:<code>0</code>]. ]
        ]

    ]

    `Error` is the only reserved builtin name in this family.
    `ErrorKind`, `ErrorLocation`, and `StackFrame` are ordinary user-available names.

    [codeblock, $code("bst"):
        missing = Error("Missing value")
        coded = Error(message = "Missing value", code = 200)
    ]
]

#[section, $markdown:
    # Returning errors

    Use `return!` to return through the error slot.

    [codeblock, $code("bst"):
        parse_number |text String| -> Int, Error!:
            if text.is_empty():
                return! Error("Missing number", 200)
            ;

            return 42
        ;
    ]

    A function with an `Error!` return slot can return either:

    - success values with `return`
    - an error value with `return!`

    This keeps success and failure paths explicit.
]

#[section, $markdown:
    # Bubbling errors

    Use a trailing `!` to bubble an error to the surrounding function.

    [codeblock, $code("bst"):
        wrapper |text String| -> Int, Error!:
            value = parse_number(text)!
            return value
        ;
    ]

    If `parse_number(text)` succeeds, execution continues normally.

    If it returns an error, the surrounding function returns that error from its own error slot.

    The surrounding function must declare a compatible `Error!` return slot.
]

#[section, $markdown:
    # Catching errors

    Use `catch` to inspect or recover from an error.

    `catch` has two forms:

    - `catch:` runs a recovery block when the error value is not needed
    - `catch |err|:` binds the error and runs a recovery block

    A catch block can recover with fallback success values by producing values with `then`.
    `then` returns expressions from the catch block to the containing assignment,
    return, or multi-bind site. Nested ordinary branch paths inside a value-producing
    catch can also target the same catch recovery block with `then`.

    [codeblock, $code("bst"):
        wrapper |text String| -> Int, Error!:
            value = parse_number(text) catch |err|:
                io.line(err.message)
                return! err
            ;

            return value
        ;
    ]

    In this example:

    - `err` is the caught error value
    - the handler body can inspect or log the error
    - `return! err` explicitly rethrows the same error through the surrounding function

    A success-producing `catch` block without `then` must terminate on
    every reachable handler path with `return` or `return!`. Otherwise the
    handled expression would have no success value to assign.
]

#[section, $markdown:
    # Recovering with fallback values

    Use `catch:` with `then` to recover from an error with fallback success values.

    [codeblock, $code("bst"):
        recover |text String| -> Int:
            value = parse_number(text) catch:
                then 0
            ;

            return value
        ;
    ]

    If `parse_number(text)` succeeds, `value` receives the parsed number.

    If it returns an error, `value` receives `0`.

    The `then` value must match the expected success type.
]

#[section, $markdown:
    # Recovering with a handler block

    Use `catch |err|:` when recovery needs to inspect or log the error.

    [codeblock, $code("bst"):
        recover |text String| -> Int:
            value = parse_number(text) catch |err|:
                io.line(err.message)
                then 0
            ;

            return value
        ;
    ]

    In this example:

    - `err` is the caught error value
    - `then 0` returns the fallback success value from the catch block
    - the handler body can inspect or log the error
    - the surrounding function does not need to return `Error!`

    The handler body runs only on the error path.

    The `then` fallback value is used after the handler body completes.
]

#[section, $markdown:
    # Overriding catch behavior

    A success-producing `catch` block can omit `then` only when the
    handler exits explicitly.

    [codeblock, $code("bst"):
        recover |text String| -> Int:
            value = parse_number(text) catch |err|:
                io.line(err.message)
                return 0
            ;

            return value
        ;
    ]

    In this example, the surrounding function returns `0` from inside the `catch` block.

    This is useful when the surrounding function does not return `Error!`, but the recovery path needs more control than a fallback expression.
]

#[section, $markdown:
    # Multiple success values

    Beanstalk supports multiple return values.

    A function can return multiple success values and one error slot.

    [codeblock, $code("bst"):
        load_user |id String| -> String, Float, Error!:
            if id.is_empty():
                return! Error("Missing user id", 1000)
            ;

            return "guest", 0.0
        ;

        name, score = load_user(id) catch |err|:
            io.line(err.message)
            then "guest", 0.0
        ;
    ]

    The `then` fallback values must match the success return list.

    If the success path returns two values, the recovery path must provide two fallback values.
]

#[section, $markdown:
    # Error handling forms

    [styled_table:
        [header: [: Form] [: Meaning] ]

        [data:
            [: [$html:<code>parse_number(text)!</code>] ]
            [: Bubble the error to the surrounding function. ]
        ]

        [data:
            [: [$html:<code>parse_number(text) catch: then 0 ;</code>] ]
            [: Use [$html:<code>0</code>] as a fallback value if an error is returned. ]
        ]

        [data:
            [: [$html:<code>parse_number(text) catch |err|: ... return! err ;</code>] ]
            [: Bind the error as [$html:<code>err</code>], run a handler block, and terminate explicitly with [$html:<code>return</code>] or [$html:<code>return!</code>]. ]
        ]

        [data:
            [: [$html:<code>parse_number(text) catch |err|: ... then 0 ;</code>] ]
            [: Bind the error as [$html:<code>err</code>], run a handler block, then recover with [$html:<code>0</code>]. ]
        ]

        [data:
            [: [$html:<code>return!</code>] ]
            [: Return an error from the current function's error slot. ]
        ]
    ]
]

#[section, $markdown:
    # Collection errors

    Collection operations use structured errors for invalid runtime cases.

    By default, `get(index)` returns a result-like value that must be handled.

    [codeblock, $code("bst"):
        my_list ~= {1, 2, 3}

        fallback = my_list.get(0) catch:
            then 0
        ;

        value = my_list.get(5) catch |err|:
            io.line(err.message)
            then fallback
        ;
    ]

    If the index is out of bounds, the error value is an `Error` with a `message` field.

    Collection methods should not silently ignore invalid operations.

    They should produce structured errors that can be caught, bubbled, or recovered from.
]

#[section, $markdown:
    # Strict runtime validation

    Collection methods should enforce strict runtime validation.

    This includes:

    - invalid receivers
    - out-of-bounds indices
    - invalid mutation targets
    - invalid remove operations
    - backend-specific runtime failures

    Compiler-owned collection methods report failures through the same fallible call path as
    user functions.

    User code still sees the same error-handling surface:

    - recover with a fallback
    - recover with a catch handler block
    - bubble the error
]

#[section, $markdown:
    # Options

    Optional values use `?`.

    An optional value represents a value that may not exist.

    [codeblock, $code("bst"):
        find_url |has_url Bool| -> String?:
            if has_url:
                return "https://nyejames.com"
            ;

            return none
        ;

        url String? = find_url(false)
    ]

    `none` is the only special option value.

    A normal value of type `T` can be used where `T?` is expected.

    `none` requires an optional type context.

    Option recovery uses explicit present-value inspection.

    [codeblock, $code("bst"):
        display_name = if maybe_name is |name| then name else "guest"
    ]

    `|name|` matches the present case and binds the inner value.
    Absence checks remain statement-friendly:

    [codeblock, $code("bst"):
        if maybe_name is none:
            io.line("missing")
        ;
    ]

    Direct option fallback syntax such as `maybe_name else then "guest"` is rejected in favour of `if option is |value| ... else ...`.

    Postfix `?` propagates absence from functions that return an optional value.

    [codeblock, $code("bst"):
        get_display_name |id String| -> String?:
            user = find_user(id)?
            return user.name
        ;
    ]
]

#[section, $markdown:
    # Errors, options, and assertions

    Errors, options, and assertions solve different problems.

    [styled_table:
        [header: [: Feature] [: Use when] [: Example] ]

        [data:
            [: [$html:<code>T?</code>] ]
            [: Absence is expected and not a failure. ]
            [: A user profile has no avatar. ]
        ]

        [data:
            [: [$html:<code>Error!</code>] ]
            [: Something failed and the caller needs failure information. ]
            [: Parsing text failed because the format was invalid. ]
        ]

        [data:
            [: [$html:<code>assert</code>] ]
            [: A required invariant is false and execution must stop. ]
            [: An index must already be in bounds before unsafe work continues. ]
        ]
    ]

    Use `?` when missing data is ordinary.

    Use `Error!` when failure details matter.

    Use `assert` for broken assumptions, not regular failures.
]

#[section, $markdown:
    # Assertions

    Assertions are different from recoverable errors.

    A failed assertion means the program should not continue.

    Assertions are intended for cases such as:

    - impossible states
    - critical invariant violations
    - debugging unexpected state
    - stopping before unsafe or invalid behavior could continue

    Assertion syntax:

    [codeblock, $code("bst"):
        assert(index < items.length)
        assert(index < items.length, "index must be in bounds")
        assert(false, "unimplemented backend path")
    ]

    `assert` is a language-owned statement intrinsic, not a function.
    It returns no value, cannot be assigned or passed as an argument, and cannot be used in template interpolation.

    Assertions are always checked in development and release builds.

    Failed assertions are unrecoverable in Beanstalk source.
    They do not return `Error!`, and `catch` cannot catch them.

    `assert(false, "message")` is the only explicit source-level spelling for an intentional unrecoverable stop for now.

    Assertion messages currently must be string literals.
    Runtime message expressions and compile-time constant messages are deferred until failure-only message evaluation is designed.

    Most expected failures should use `Error!` instead.
]

#[section, $markdown:
    # Assertion diagnostics

    [styled_table:
        [header: [: Invalid form] [: Diagnostic direction] ]

        [data:
            [: [$html:<code>assert(1)</code>] ]
            [: The condition must be [$html:<code>Bool</code>]. ]
        ]

        [data:
            [: [$html:<code>assert()</code>] ]
            [: The assertion condition is required. ]
        ]

        [data:
            [: [$html:<code>assert(true, value)</code>] ]
            [: Assertion messages must be string literals for now. ]
        ]

        [data:
            [: [$html:<code>value = assert(true)</code>] ]
            [: [$html:<code>assert</code>] is statement-only and cannot be used as a value. ]
        ]

        [data:
            [: [$html:<code>assert(true) catch:</code>] ]
            [: Assertion failure is not a recoverable [$html:<code>Error!</code>] path. ]
        ]
    ]
]

#[section, $markdown:
    # Full example

    [codeblock, $code("bst"):
        parse_number |text String| -> Int, Error!:
            if text.is_empty():
                return! Error("Missing number", 200)
            ;

            return 42
        ;

        wrapper |text String| -> Int, Error!:
            value = parse_number(text)!
            return value
        ;

        log_then_rethrow |text String| -> Int, Error!:
            value = parse_number(text) catch |err|:
                io.line(err.message)
                return! err
            ;

            return value
        ;

        recover |text String| -> Int:
            value = parse_number(text) catch |err|:
                io.line(err.message)
                then 0
            ;

            return value
        ;

        fallback_value = parse_number("") catch:
            then 0
        ;
    ]

    This example shows the core call-site choices:

    - bubble with `parse_number(text)!`
    - recover with `parse_number(text) catch: then 0 ;`
    - catch, inspect, and explicitly rethrow with `return!`
    - catch, inspect, and recover with `catch |err|: ... then 0 ;`
]

#[section, $markdown:
    # Diagnostics

    Error handling should produce clear diagnostics.

    Useful diagnostic cases include:

    - calling an error-returning function without handling the error
    - using trailing `!` in a function with no compatible error return slot
    - using success-producing `catch` without `then` when the handler can fall through
    - using `then` fallback values with the wrong type or wrong count
    - using `return!` in a function that does not return `Error!`
    - using `none` without optional type context
    - using `assert` in expression position
    - using `assert` with a non-`Bool` condition
    - using `assert` with `!` or `catch`

    Error handling is part of the type system.

    It should not be treated as a backend-only runtime behavior.
]

#[section, $markdown:
    # Summary

    Beanstalk separates three kinds of failure or absence:

    - `Error!` for recoverable errors with structured information
    - `?` and `none` for optional values
    - `assert` for always-checked unrecoverable invariant failures

    The core rules are:

    - functions that can fail mark one return slot with `!`
    - error-returning calls must be handled
    - `return!` returns an error
    - trailing `!` bubbles an error
    - `catch:` with `then` recovers with fallback success values
    - `catch |err|: ... return! err ;` catches, handles, and explicitly rethrows
    - `catch |err|: ... then fallback ;` catches, handles, and recovers
    - failed assertions are not catchable and should not be used for normal control flow

    The result is explicit, typed, and compiler-checkable error handling.
]
