import @html {center}
import @styles/docs {navbar, title, section, styled_table, header, data, codeblock, codesnippet, theme_head}

# page_title = "Coroutines and Async in Beanstalk"
# page_description = "Design draft for structured async, coroutines, channels, and explicit suspension in Beanstalk."
# page_head = theme_head

#[navbar]
#[title, center: COROUTINES AND ASYNC IN BEANSTALK]

#[section, $markdown:
    **Design draft:** This page is a future-facing design document for language and compiler planning.
    It is not final user-facing documentation, and the syntax, semantics, and implementation details may change before this feature is implemented.

    Beanstalk approaches async and concurrency by making **control flow explicit** and **communication safe by default**.

    Instead of hiding async behind keywords like [codesnippet:async] / [codesnippet:await] or complex runtime abstractions,
    Beanstalk uses a small set of primitives that make data flow, suspension, and concurrency visible in the syntax.

    At a high level:

    - Coroutines are lightweight, cooperative tasks
    - Coroutines communicate using typed channels
    - Concurrency is structured and scoped
    - Suspension points are visible in code
    - The compiler can enforce safety rules around shared data, references, and task lifetimes

    The goal is async code that remains readable, predictable, and analyzable.
]

#[section, $markdown:
    # Core idea

    Async in Beanstalk is built around three concepts:

    [styled_table:
        [header: [: Concept] [: Syntax] [: Meaning] ]

        [data:
            [: Channels ]
            [: [codesnippet:Channel(Int)] ]
            [: Create typed communication endpoints ]
        ]

        [data:
            [: Send ]
            [: [codesnippet:>>] ]
            [: Move or pass a value into a channel ]
        ]

        [data:
            [: Receive ]
            [: [codesnippet:<<] ]
            [: Wait for a value from a channel ]
        ]

        [data:
            [: Coroutine ]
            [: [codesnippet:spawn] ]
            [: Start cooperative work inside an async scope ]
        ]

        [data:
            [: Suspension ]
            [: [codesnippet:yield] ]
            [: Explicitly give control back to the scheduler ]
        ]

        [data:
            [: Scope ]
            [: [codesnippet:async:] ]
            [: Bound the lifetime of spawned coroutines ]
        ]
    ]

    The intended model is:

    > Async is structured control flow plus safe message passing.
]

#[section, $markdown:
    # Channels

    Coroutines should not share mutable state by default.

    Instead, they communicate by sending values through typed channels.

    A channel is created with [codesnippet:Channel], which takes a carried value type and returns two endpoints:

    [codeblock, $code("bst"):
        reader, writer = Channel(Int)
    ]

    In this example:

    - [codesnippet:writer] is the sending endpoint
    - [codesnippet:reader] is the receiving endpoint
    - the channel carries values of type [codesnippet:Int]

    You can think of this as a one-way pipe for values of a specific type.
]

#[section, $markdown:
    # Sending values

    Values are sent into a channel with [codesnippet:>>].

    [codeblock, $code("bst"):
        value >> writer
    ]

    The value flows to the right into the channel.

    Sending may suspend the current coroutine if the receiver is not ready.

    This is an intentional visible suspension point.
]

#[section, $markdown:
    # Receiving values

    Values are received from a channel with [codesnippet:<<].

    [codeblock, $code("bst"):
        value << reader
    ]

    The value flows left out of the channel and is assigned to [codesnippet:value].

    Receiving may suspend the current coroutine until a value is available.

    Like sending, this is a visible suspension point.
]

#[section, $markdown:
    # Coroutines

    A coroutine is a function that can suspend and resume execution.

    Coroutines are started with [codesnippet:spawn] and must run inside an [codesnippet:async] scope.

    Example coroutine functions:

    [codeblock, $code("bst"):
        producer |writer ~Channel[Int]|:
            loop 0 to 10 |i|:
                i >> writer
                yield
            ;
        ;

        consumer |reader ~Channel[Int]|:
            loop:
                value << reader
                io(value)
            ;
        ;
    ]

    In this example:

    - [codesnippet:producer] sends integers into the channel
    - [codesnippet:consumer] receives and prints them
    - neither coroutine shares mutable memory directly
    - all interaction happens through the channel
]

#[section, $markdown:
    # Yield

    [codesnippet:yield] explicitly suspends the current coroutine and allows other coroutines to run.

    [codeblock, $code("bst"):
        yield
    ]

    This is a cooperative pause point.

    It does not block the whole program. It tells the scheduler that the current coroutine is ready to let something else run.

    Example:

    [codeblock, $code("bst"):
        loop 0 to 1_000_000 |i|:
            i >> writer

            if i % 128 is 0:
                yield
            ;
        ;
    ]

    Here, [codesnippet:yield] prevents one long-running coroutine from starving others.

    Suspension is always visible in the code.

    There are no hidden pause points.
]

#[section, $markdown:
    # Structured async scopes

    Coroutines run inside an [codesnippet:async] scope.

    [codeblock, $code("bst"):
        async:
            ...
        ;
    ]

    An [codesnippet:async] scope means:

    - coroutines may be spawned inside the scope
    - suspension is allowed inside the scope
    - channel send and receive operations may suspend inside the scope
    - spawned coroutines are bounded by the scope
    - no detached background task can silently outlive its data

    Example:

    [codeblock, $code("bst"):
        async:
            reader, writer = Channel(Int)

            spawn producer(writer)
            spawn consumer(reader)

            loop:
                yield
            ;
        ;
    ]

    This makes concurrency structured and bounded.

    You can see where async work starts, where it may suspend, and where it must be contained.
]

#[section, $markdown:
    # Why not viral async types?

    Beanstalk should avoid making async infect every function type by default.

    The draft model does not rely on a special [codesnippet, "Future[T]"] return type everywhere, and it does not require every caller to become async just because one operation may suspend.

    Instead:

    - suspension is only valid in async-capable contexts
    - suspension points are explicit
    - channels make communication visible
    - spawned work is scoped
    - the compiler can reject unsafe use across scope boundaries

    The goal is not to hide async.

    The goal is to make async local and visible.
]

#[section, $markdown:
    # Safety model

    The async model should preserve Beanstalk's normal safety rules.

    Async code should not allow:

    - data races
    - dangling references into completed scopes
    - detached tasks that keep using local state
    - hidden mutable sharing between coroutines
    - unbounded background work by default

    Channels are the main communication primitive because they give the compiler a clear place to reason about ownership, borrowing, and transfer.

    Mutable shared state may be possible later through specific safe abstractions, but it should not be the default async model.
]

#[section, $markdown:
    # Full example

    [codeblock, $code("bst"):
        producer |writer ~Channel[Int]|:
            loop 0 to 10 |i|:
                i >> writer
                yield
            ;
        ;

        consumer |reader ~Channel[Int]|:
            loop:
                value << reader
                io(value)
            ;
        ;

        async:
            reader, writer = Channel(Int)

            spawn producer(writer)
            spawn consumer(reader)

            loop:
                yield
            ;
        ;
    ]

    This example has three visible forms of async behavior:

    - [codesnippet:spawn] starts concurrent work
    - [codesnippet:>>] and [codesnippet:<<] communicate through the channel
    - [codesnippet:yield] explicitly gives the scheduler a chance to run other work
]

#[section, $markdown:
    # Compiler lowering

    The exact lowering strategy is not final.

    A future compiler implementation may lower coroutines into state machines.

    Conceptually, each coroutine needs to preserve:

    - its current execution point
    - its live local values
    - pending channel operations
    - any values that cross suspension points
    - enough ownership and borrow information to remain memory safe

    This fits Beanstalk's broader compiler direction:

    - AST validates syntax and types
    - HIR makes control flow explicit
    - borrow validation checks safe access
    - backend lowering decides how coroutines are represented for the target runtime

    Async should be represented semantically before backend-specific lowering.
]

#[section, $markdown:
    # Backend considerations

    This design depends on backend capabilities.

    Important open questions include:

    - how WebAssembly stack switching evolves
    - how browsers expose scheduling and suspension
    - how native Wasm runtimes expose async host calls
    - how Beanstalk should integrate with browser event loops
    - how game engines or embedded hosts should drive scheduling
    - whether early JS output should use generator-style lowering, explicit state machines, or host promises

    The user-facing syntax should remain stable even if backend strategies differ.
]

#[section, $markdown:
    # Deferred design questions

    Several details should be deliberately deferred until the basic model is proven:

    - cancellation semantics
    - channel buffering
    - channel close behavior
    - task failure propagation
    - timeout syntax
    - select-style waiting on multiple channels
    - async host function imports
    - interaction with closures and captured mutable access
    - whether [codesnippet, "Channel[Int]"] remains the final channel type syntax

    These should be designed as extensions, not packed into the first implementation.
]

#[section, $markdown:
    # Current status

    This design is not implemented yet.

    It is intended as a future reference for language design and compiler implementation planning.

    The first implementation should likely focus on:

    - parsing and rejecting async syntax deliberately
    - defining the HIR representation for suspension-capable control flow
    - validating what values may live across suspension points
    - modelling channel endpoints as typed values
    - lowering a minimal cooperative coroutine model in one backend

    The important part is to keep the source model explicit and structured.
]

#[section, $markdown:
    # Summary

    Async in Beanstalk is intended to be:

    - explicit: suspension is visible
    - structured: spawned work is scoped
    - safe: communication goes through typed channels
    - analyzable: control flow and data flow are compiler-visible
    - backend-flexible: JS, Wasm, and host runtimes can lower the same semantics differently

    The core primitives are:

    - [codesnippet:Channel(T)] creates typed channel endpoints
    - [codesnippet:>>] sends values
    - [codesnippet:<<] receives values
    - [codesnippet:yield] suspends the current coroutine
    - [codesnippet:spawn] starts a coroutine inside an async scope
    - [codesnippet:async:] defines the structured concurrency boundary

    The core rule is:

    > Async work must be visible, scoped, and safe by construction.
]