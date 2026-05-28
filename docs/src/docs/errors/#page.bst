import @html {center}
import @styles/docs {navbar, title, section, styled_table, header, data, codeblock, codesnippet, theme_head}

page_title #= "Errors, Options, and Assertions in Beanstalk"
page_description #= "Error values, error propagation, option values, and always-checked assertions in Beanstalk."
page_head #= theme_head

#[navbar]
#[title, center: ERRORS, OPTIONS, AND ASSERTIONS IN BEANSTALK]

#[section, $markdown:
    This page tracks Beanstalk's current error-handling surface.
    The public [codesnippet:Error] value shape and [codesnippet:catch] forms shown here are current.

    Errors are values in Beanstalk.

    Functions that can fail return an [codesnippet:Error] value through a dedicated error return slot.

    That error slot is marked with [codesnippet:!].

    Any call that can return an error must handle it.

    The core rule is:

    > If a function can return an error, the caller must either catch the error or bubble it upward.
]

#[section, $markdown:
    # Core syntax

    Error-returning functions mark one return slot with [codesnippet:!].

    [codeblock, $code("bst"):
        parse_number |text String| -> Int, Error!:
            if text.is_empty():
                return! Error("Missing number", 200)
            ;

            return 42
        ;
    ]

    In this example:

    - [codesnippet:Int] is the success value
    - [codesnippet:Error!] is the error slot
    - [codesnippet:return!] returns through the error slot
    - normal [codesnippet:return] returns through the success path

    The [codesnippet:!] marker does not mean panic.

    It means this function has a typed error path that callers must handle.
    Expected failures should use [codesnippet:Error!] and [codesnippet:catch].
    Assertions are for unrecoverable invariants and are the current explicit panic path.

    [codesnippet:catch:] and [codesnippet, "catch |err|:"] are the supported result recovery forms.
    [codesnippet:then] produces local values for a surrounding value-receiving site. Value-producing [codesnippet:if] and full match receivers reuse that mechanism now; catch recovery still uses it inside catch handlers until catch is refactored onto the shared value-block model.
    Public first-class [codesnippet:Result] values and raw fallible carrier storage remain deferred; use [codesnippet:!] return slots and call-site handling instead.
]

#[section, $markdown:
    # The Error type

    [codesnippet:Error] is a builtin language type.

    It carries a human-readable message and an integer code.

    Publicly, [codesnippet:Error] contains:

    [styled_table:
        [header: [: Field] [: Type] [: Meaning] ]

        [data:
            [: [codesnippet:message] ]
            [: [codesnippet:String] ]
            [: Human-readable explanation ]
        ]

        [data:
            [: [codesnippet:code] ]
            [: [codesnippet:Int] ]
            [: Stable numeric error code. Defaults to [codesnippet:0]. ]
        ]

    ]

    [codesnippet:Error] is the only reserved builtin name in this family.
    [codesnippet:ErrorKind], [codesnippet:ErrorLocation], and [codesnippet:StackFrame] are ordinary user-available names.

    [codeblock, $code("bst"):
        missing = Error("Missing value")
        coded = Error(message = "Missing value", code = 200)
    ]
]

#[section, $markdown:
    # Returning errors

    Use [codesnippet:return!] to return through the error slot.

    [codeblock, $code("bst"):
        parse_number |text String| -> Int, Error!:
            if text.is_empty():
                return! Error("Missing number", 200)
            ;

            return 42
        ;
    ]

    A function with an [codesnippet:Error!] return slot can return either:

    - success values with [codesnippet:return]
    - an error value with [codesnippet:return!]

    This keeps success and failure paths explicit.
]

#[section, $markdown:
    # Bubbling errors

    Use a trailing [codesnippet:!] to bubble an error to the surrounding function.

    [codeblock, $code("bst"):
        wrapper |text String| -> Int, Error!:
            value = parse_number(text)!
            return value
        ;
    ]

    If [codesnippet:parse_number(text)] succeeds, execution continues normally.

    If it returns an error, the surrounding function returns that error from its own error slot.

    The surrounding function must declare a compatible [codesnippet:Error!] return slot.
]

#[section, $markdown:
    # Catching errors

    Use [codesnippet:catch] to inspect or recover from an error.

    [codesnippet:catch] has two forms:

    - [codesnippet:catch:] runs a recovery block when the error value is not needed
    - [codesnippet, "catch |err|:"] binds the error and runs a recovery block

    A catch block can recover with fallback success values by producing values with [codesnippet:then].
    [codesnippet:then] returns expressions from the catch block to the containing assignment,
    return, or multi-bind site. Nested ordinary branch paths inside a value-producing
    catch can also target the same catch recovery block with [codesnippet:then].

    [codeblock, $code("bst"):
        wrapper |text String| -> Int, Error!:
            value = parse_number(text) catch |err|:
                io(err.message)
                return! err
            ;

            return value
        ;
    ]

    In this example:

    - [codesnippet:err] is the caught error value
    - the handler body can inspect or log the error
    - [codesnippet:return! err] explicitly rethrows the same error through the surrounding function

    A success-producing [codesnippet:catch] block without [codesnippet:then] must terminate on
    every reachable handler path with [codesnippet:return] or [codesnippet:return!]. Otherwise the
    handled expression would have no success value to assign.
]

#[section, $markdown:
    # Recovering with fallback values

    Use [codesnippet:catch:] with [codesnippet:then] to recover from an error with fallback success values.

    [codeblock, $code("bst"):
        recover |text String| -> Int:
            value = parse_number(text) catch:
                then 0
            ;

            return value
        ;
    ]

    If [codesnippet:parse_number(text)] succeeds, [codesnippet:value] receives the parsed number.

    If it returns an error, [codesnippet:value] receives [codesnippet:0].

    The [codesnippet:then] value must match the expected success type.
]

#[section, $markdown:
    # Recovering with a handler block

    Use [codesnippet, "catch |err|:"] when recovery needs to inspect or log the error.

    [codeblock, $code("bst"):
        recover |text String| -> Int:
            value = parse_number(text) catch |err|:
                io(err.message)
                then 0
            ;

            return value
        ;
    ]

    In this example:

    - [codesnippet:err] is the caught error value
    - [codesnippet, "then 0"] returns the fallback success value from the catch block
    - the handler body can inspect or log the error
    - the surrounding function does not need to return [codesnippet:Error!]

    The handler body runs only on the error path.

    The [codesnippet:then] fallback value is used after the handler body completes.
]

#[section, $markdown:
    # Overriding catch behavior

    A success-producing [codesnippet:catch] block can omit [codesnippet:then] only when the
    handler exits explicitly.

    [codeblock, $code("bst"):
        recover |text String| -> Int:
            value = parse_number(text) catch |err|:
                io(err.message)
                return 0
            ;

            return value
        ;
    ]

    In this example, the surrounding function returns [codesnippet:0] from inside the [codesnippet:catch] block.

    This is useful when the surrounding function does not return [codesnippet:Error!], but the recovery path needs more control than a fallback expression.
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
            io(err.message)
            then "guest", 0.0
        ;
    ]

    The [codesnippet:then] fallback values must match the success return list.

    If the success path returns two values, the recovery path must provide two fallback values.
]

#[section, $markdown:
    # Error handling forms

    [styled_table:
        [header: [: Form] [: Meaning] ]

        [data:
            [: [codesnippet, "parse_number(text)!"] ]
            [: Bubble the error to the surrounding function. ]
        ]

        [data:
            [: [codesnippet, "parse_number(text) catch: then 0 ;"] ]
            [: Use [codesnippet:0] as a fallback value if an error is returned. ]
        ]

        [data:
            [: [codesnippet, "parse_number(text) catch |err|: ... return! err ;"] ]
            [: Bind the error as [codesnippet:err], run a handler block, and terminate explicitly with [codesnippet:return] or [codesnippet:return!]. ]
        ]

        [data:
            [: [codesnippet, "parse_number(text) catch |err|: ... then 0 ;"] ]
            [: Bind the error as [codesnippet:err], run a handler block, then recover with [codesnippet:0]. ]
        ]

        [data:
            [: [codesnippet:return!] ]
            [: Return an error from the current function's error slot. ]
        ]
    ]
]

#[section, $markdown:
    # Collection errors

    Collection operations use structured errors for invalid runtime cases.

    By default, [codesnippet:get(index)] returns a result-like value that must be handled.

    [codeblock, $code("bst"):
        my_list ~= {1, 2, 3}

        fallback = my_list.get(0) catch:
            then 0
        ;

        value = my_list.get(5) catch |err|:
            io(err.message)
            then fallback
        ;
    ]

    If the index is out of bounds, the error value is an [codesnippet:Error] with a [codesnippet:message] field.

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

    Optional values use [codesnippet:?].

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

    [codesnippet:none] is the only special option value.

    A normal value of type [codesnippet:T] can be used where [codesnippet:T?] is expected.

    [codesnippet:none] requires an optional type context.

    Option recovery uses explicit present-value inspection.

    [codeblock, $code("bst"):
        display_name = if maybe_name is |name| then name else "guest"
    ]

    [codesnippet, "|name|"] matches the present case and binds the inner value.
    Absence checks remain statement-friendly:

    [codeblock, $code("bst"):
        if maybe_name is none:
            io("missing")
        ;
    ]

    Direct option fallback syntax such as [codesnippet, "maybe_name else then \"guest\""] is rejected in favour of [codesnippet, "if option is |value| ... else ..."].

    Postfix [codesnippet:?] propagates absence from functions that return an optional value.

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
            [: [codesnippet:T?] ]
            [: Absence is expected and not a failure. ]
            [: A user profile has no avatar. ]
        ]

        [data:
            [: [codesnippet:Error!] ]
            [: Something failed and the caller needs failure information. ]
            [: Parsing text failed because the format was invalid. ]
        ]

        [data:
            [: [codesnippet:assert] ]
            [: A required invariant is false and execution must stop. ]
            [: An index must already be in bounds before unsafe work continues. ]
        ]
    ]

    Use [codesnippet:?] when missing data is ordinary.

    Use [codesnippet:Error!] when failure details matter.

    Use [codesnippet:assert] for broken assumptions, not regular failures.
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

    [codesnippet:assert] is a language-owned statement intrinsic, not a function.
    It returns no value, cannot be assigned or passed as an argument, and cannot be used in template interpolation.

    Assertions are always checked in development and release builds.

    Failed assertions are unrecoverable in Beanstalk source.
    They do not return [codesnippet:Error!], and [codesnippet:catch] cannot catch them.

    [codesnippet, "assert(false, \"message\")"] is the only explicit source-level spelling for an intentional unrecoverable stop for now.

    Assertion messages currently must be string literals.
    Runtime message expressions and compile-time constant messages are deferred until failure-only message evaluation is designed.

    Most expected failures should use [codesnippet:Error!] instead.
]

#[section, $markdown:
    # Assertion diagnostics

    [styled_table:
        [header: [: Invalid form] [: Diagnostic direction] ]

        [data:
            [: [codesnippet:assert(1)] ]
            [: The condition must be [codesnippet:Bool]. ]
        ]

        [data:
            [: [codesnippet:assert()] ]
            [: The assertion condition is required. ]
        ]

        [data:
            [: [codesnippet:assert(true, value)] ]
            [: Assertion messages must be string literals for now. ]
        ]

        [data:
            [: [codesnippet, "value = assert(true)"] ]
            [: [codesnippet:assert] is statement-only and cannot be used as a value. ]
        ]

        [data:
            [: [codesnippet, "assert(true) catch:"] ]
            [: Assertion failure is not a recoverable [codesnippet:Error!] path. ]
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
                io(err.message)
                return! err
            ;

            return value
        ;

        recover |text String| -> Int:
            value = parse_number(text) catch |err|:
                io(err.message)
                then 0
            ;

            return value
        ;

        fallback_value = parse_number("") catch:
            then 0
        ;
    ]

    This example shows the core call-site choices:

    - bubble with [codesnippet, "parse_number(text)!"]
    - recover with [codesnippet, "parse_number(text) catch: then 0 ;"]
    - catch, inspect, and explicitly rethrow with [codesnippet:return!]
    - catch, inspect, and recover with [codesnippet, "catch |err|: ... then 0 ;"]
]

#[section, $markdown:
    # Diagnostics

    Error handling should produce clear diagnostics.

    Useful diagnostic cases include:

    - calling an error-returning function without handling the error
    - using trailing [codesnippet:!] in a function with no compatible error return slot
    - using success-producing [codesnippet:catch] without [codesnippet:then] when the handler can fall through
    - using [codesnippet:then] fallback values with the wrong type or wrong count
    - using [codesnippet:return!] in a function that does not return [codesnippet:Error!]
    - using [codesnippet:none] without optional type context
    - using [codesnippet:assert] in expression position
    - using [codesnippet:assert] with a non-[codesnippet:Bool] condition
    - using [codesnippet:assert] with [codesnippet:!] or [codesnippet:catch]

    Error handling is part of the type system.

    It should not be treated as a backend-only runtime behavior.
]

#[section, $markdown:
    # Summary

    Beanstalk separates three kinds of failure or absence:

    - [codesnippet:Error!] for recoverable errors with structured information
    - [codesnippet:?] and [codesnippet:none] for optional values
    - [codesnippet:assert] for always-checked unrecoverable invariant failures

    The core rules are:

    - functions that can fail mark one return slot with [codesnippet:!]
    - error-returning calls must be handled
    - [codesnippet:return!] returns an error
    - trailing [codesnippet:!] bubbles an error
    - [codesnippet:catch:] with [codesnippet:then] recovers with fallback success values
    - [codesnippet:catch |err|: ... return! err ;] catches, handles, and explicitly rethrows
    - [codesnippet:catch |err|: ... then fallback ;] catches, handles, and recovers
    - failed assertions are not catchable and should not be used for normal control flow

    The result is explicit, typed, and compiler-checkable error handling.
]
