import @html {center}
import @styles/docs {navbar, title, section, styled_table, header, data, codeblock, codesnippet, theme_head}

# page_title = "Errors and Panics in Beanstalk"
# page_description = "Design draft for error values, error propagation, option values, and runtime panics in Beanstalk."
# page_head = theme_head

#[navbar]
#[title, center: ERRORS AND PANICS IN BEANSTALK]

#[section, $markdown:
    **Design draft:** This page is a future-facing design document for language and compiler planning.
    It is not final user-facing documentation, and the syntax, semantics, and implementation details may change before this feature is implemented.

    Errors are values in Beanstalk.

    Functions that can fail return an [codesnippet:Error] value through a dedicated error return slot.

    That error slot is marked with [codesnippet:!].

    Any call that can return an error must handle it.

    The core rule is:

    > If a function can return an error, the caller must either handle the error or bubble it upward.
]

#[section, $markdown:
    # Core syntax

    Error-returning functions mark one return slot with [codesnippet:!].

    [codeblock, $code("bst"):
        parse_number |text String| -> Int, Error!:
            if text.is_empty():
                return! Error("Parse", "int.empty", "Missing number")
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
]

#[section, $markdown:
    # The Error type

    [codesnippet:Error] is a builtin language type.

    It is intended to carry structured failure information instead of only a string.

    Conceptually, [codesnippet:Error] contains:

    [styled_table:
        [header: [: Field] [: Type] [: Meaning] ]

        [data:
            [: [codesnippet:kind] ]
            [: [codesnippet:ErrorKind] ]
            [: Broad category of failure ]
        ]

        [data:
            [: [codesnippet:code] ]
            [: [codesnippet:String] ]
            [: Stable machine-readable error code ]
        ]

        [data:
            [: [codesnippet:message] ]
            [: [codesnippet:String] ]
            [: Human-readable explanation ]
        ]

        [data:
            [: [codesnippet:location] ]
            [: [codesnippet:ErrorLocation?] ]
            [: Optional source or runtime location ]
        ]

        [data:
            [: [codesnippet:trace] ]
            [: [codesnippet:{StackFrame}?] ]
            [: Optional stack trace or call trace ]
        ]
    ]

    [codesnippet:Error], [codesnippet:ErrorKind], [codesnippet:ErrorLocation], and [codesnippet:StackFrame] are reserved builtin symbols.
]

#[section, $markdown:
    # Returning errors

    Use [codesnippet:return!] to return through the error slot.

    [codeblock, $code("bst"):
        parse_number |text String| -> Int, Error!:
            if text.is_empty():
                return! Error("Parse", "int.empty", "Missing number")
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
    # Fallback values

    Use [codesnippet:!] followed by a fallback value to recover from an error.

    [codeblock, $code("bst"):
        value = parse_number("") ! 0
    ]

    If [codesnippet:parse_number] succeeds, [codesnippet:value] receives the parsed number.

    If [codesnippet:parse_number] returns an error, [codesnippet:value] receives [codesnippet:0].

    The fallback value must match the expected success type.
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

    The surrounding function must also declare a compatible [codesnippet:Error!] return slot.
]

#[section, $markdown:
    # Named error handlers

    Use [codesnippet:err!] to bind the error value inside a handler block.

    [codeblock, $code("bst"):
        recover |text String| -> Int:
            value = parse_number(text) err! 0:
                io(err.message)
            ;

            return value
        ;
    ]

    In this example:

    - [codesnippet:err] is the error value
    - [codesnippet:0] is the fallback success value
    - the handler body can inspect or log the error
    - the surrounding function does not need to return [codesnippet:Error!]

    Named handlers are useful when recovery needs more than a simple fallback expression.
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
            [: [codesnippet, "parse_number(text) ! 0"] ]
            [: Use [codesnippet:0] as a fallback value if an error is returned. ]
        ]

        [data:
            [: [codesnippet, "parse_number(text) err! 0: ... ;"] ]
            [: Bind the error as [codesnippet:err], run a handler block, then use a fallback value. ]
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

        fallback = my_list.length() ! 0

        value = my_list.get(5) err! fallback:
            io("Index out of bounds error: ", err.message)
        ;
    ]

    If the index is out of bounds, the error value is an [codesnippet:Error] with a [codesnippet:message] field.

    Collection methods should not silently ignore invalid operations.

    They should produce structured errors that can be handled, bubbled, or recovered from.
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

    The backend handles propagation automatically for compiler-owned collection methods.

    User code still sees the same error-handling surface:

    - recover with a fallback
    - recover with a named handler
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
]

#[section, $markdown:
    # Errors vs options

    Errors and options solve different problems.

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
    ]

    Use [codesnippet:?] when missing data is ordinary.

    Use [codesnippet:Error!] when failure details matter.
]

#[section, $markdown:
    # Panics

    Panics are different from recoverable errors.

    A panic means the program should not continue.

    Panics are intended for cases such as:

    - runtime assertions
    - impossible states
    - critical invariant violations
    - debugging unexpected state
    - stopping before unsafe or invalid behavior could continue

    Panic syntax:

    [codeblock, $code("bst"):
        #panic "Message about the panic"
    ]

    Panics should be rare.

    Most expected failures should use [codesnippet:Error!] instead.
]

#[section, $markdown:
    # Errors vs panics

    [styled_table:
        [header: [: Mechanism] [: Meaning] [: Caller responsibility] ]

        [data:
            [: [codesnippet:Error!] ]
            [: Recoverable failure returned as a value. ]
            [: Must handle, recover, or bubble. ]
        ]

        [data:
            [: [codesnippet:T?] ]
            [: Optional absence of a value. ]
            [: Must handle according to optional-value rules. ]
        ]

        [data:
            [: [codesnippet:#panic] ]
            [: Runtime stop caused by an unrecoverable condition. ]
            [: Should not be used for normal control flow. ]
        ]
    ]

    Use panics for broken assumptions, not regular failures.
]

#[section, $markdown:
    # Full example

    [codeblock, $code("bst"):
        parse_number |text String| -> Int, Error!:
            if text.is_empty():
                return! Error("Parse", "int.empty", "Missing number")
            ;

            return 42
        ;

        wrapper |text String| -> Int, Error!:
            value = parse_number(text)!
            return value
        ;

        recover |text String| -> Int:
            value = parse_number(text) err! 0:
                io(err.message)
            ;

            return value
        ;

        fallback_value = parse_number("") ! 0
    ]

    This example shows the three core call-site choices:

    - bubble with [codesnippet, "parse_number(text)!"]
    - recover with [codesnippet, "parse_number(text) ! 0"]
    - handle and recover with [codesnippet:err!]
]

#[section, $markdown:
    # Diagnostics

    Error handling should produce clear diagnostics.

    Useful diagnostic cases include:

    - calling an error-returning function without handling the error
    - using [codesnippet:!] in a function with no compatible error return slot
    - using a fallback value with the wrong type
    - using [codesnippet:return!] in a function that does not return [codesnippet:Error!]
    - using [codesnippet:none] without optional type context
    - using [codesnippet:#panic] where only declarations are allowed

    Error handling is part of the type system.

    It should not be treated as a backend-only runtime behavior.
]

#[section, $markdown:
    # Summary

    Beanstalk separates three kinds of failure or absence:

    - [codesnippet:Error!] for recoverable errors with structured information
    - [codesnippet:?] and [codesnippet:none] for optional values
    - [codesnippet:#panic] for unrecoverable runtime stops

    The core rules are:

    - functions that can fail mark one return slot with [codesnippet:!]
    - error-returning calls must be handled
    - [codesnippet:return!] returns an error
    - trailing [codesnippet:!] bubbles an error
    - [codesnippet:! fallback] recovers with a fallback value
    - [codesnippet:err! fallback:] creates a named error handler
    - panics should not be used for normal control flow

    The result is explicit, typed, and compiler-checkable error handling.
]