import @html {center}
import @styles/docs {navbar, title, section, codeblock, theme_head}

page_title #= "Reactivity in Beanstalk Templates"
page_description #= "Current support for explicit reactive sources and template subscriptions in Beanstalk."
page_head #= theme_head

#[navbar]
#[title, center: REACTIVITY IN BEANSTALK TEMPLATES]

#[section, $markdown:
    Reactivity V1 is Beanstalk's constrained UI-oriented alternative to adding general closures, anonymous function values, generic function values, or higher-order polymorphism.
    It is built from explicit reactive sources, explicit template subscriptions, and backend-owned runtime updates.

    V1 syntax:

    - reactive declarations use `$Type` or `$=`
    - reactive parameters use `$Type`
    - template subscriptions use `["$(source)"]`
    - ordinary `["[source]"]` template captures remain snapshot reads

    V1 does not include event/action/effect syntax, field/path subscriptions, expression dependency tracking, reactive IO sinks, fine-grained DOM updates, or HTML-Wasm runtime support.
    The current live backend sink is top-level runtime HTML fragments in the HTML-JS builder.
]

#[section, $markdown:
    # Reactive sources

    A reactive declaration creates stable mutation-capable storage in the declaring runtime scope.

    [codeblock, $code("bst"):
        count $Int = 0
        message $String = [:Hello]

        enabled $= true
    ]

    `$Type` is reactive access syntax, not a user-facing wrapper type. The value still has the underlying semantic type:

    - `count $Int` stores an `Int`
    - `message $String` stores a `String`
    - `items ${String}` stores a `{String}`
    - `scores ${String = Int}` stores a `{String = Int}`

    Reactive identity belongs to the binding/source metadata. It is not a separate `TypeId`.
]

#[section, $markdown:
    # Reads and subscriptions

    Ordinary reads snapshot the current value. In templates, `["[source]"]` is still a normal snapshot capture.

    A live subscription uses `["$(source)"]` in a template head or capture position. V1 accepts only one bare source identifier.

    - `["[source]"]` snapshots the current value
    - `["$(source)"]` creates a read-only subscription to a stable reactive source

    `["$(source)"]` captures stable source identity and dependency metadata. It does not capture a mutable borrow, copy the value, or run a computed expression.
]

#[section, $markdown:
    # Subscription restrictions

    V1 deliberately keeps subscriptions narrow.

    Valid:

    [codeblock, $code("bst"):
        count $Int = 0
        subscription = ["$(count)"]
    ]

    Deferred or invalid:

    [codeblock, $code("bst"):
        ["$(source.field)"]
        ["$(items.get(0)!)"]
        ["$(get_count())"]
        ["$(source + 1)"]
        ["$(~source)"]
        ["$()"]
        ["$(a, b)"]
    ]

    `["$("]` is recognized only when `$` is immediately followed by `(`. `$ (source)` is not subscription syntax.

    `["$(source)"]` is invalid in compile-time templates, constants, ordinary expression positions, and non-template syntax positions.
]

#[section, $markdown:
    # Function boundaries

    A function that needs to preserve reactive source identity asks for a reactive parameter.

    [codeblock, $code("bst"):
        counter_view |count $Int| -> String:
            return [: Count subscription: ["$(count)"]]
        ;
    ]

    A `$T` parameter is a read/subscription handle to an existing reactive source. It does not grant mutation permission.

    A reactive source passed to an ordinary `T` parameter is a snapshot read, except that reactive template string values may keep their template metadata when passed through ordinary `String` parameters and inserted into another template.

    A non-reactive value cannot satisfy a `$T` parameter.
]

#[section, $markdown:
    # Mutation and invalidation

    Assignment to a reactive declaration updates the same stable source and invalidates subscribers.

    [codeblock, $code("bst"):
        count $Int = 0

        count = 1
    ]

    Mutating through a reactive source invalidates the whole source in V1:

    - assignment
    - field writes
    - collection or hashmap mutation
    - mutable write-through calls

    V1 has no field, item, or path-level invalidation. Those are follow-up backend and language work.
]

#[section, $markdown:
    # Relationship to borrowing

    Reactive subscriptions are not mutable borrows and should not block later ordinary mutable access.

    A subscription observes a source. Mutation still follows ordinary Beanstalk rules, including explicit `~` for mutable/exclusive call access.

    The borrow checker treats subscriptions as read-only source dependency metadata, not active borrow lifetimes.
]

#[section, $markdown:
    # Reactive template strings

    A runtime template containing a subscription still has language type `String`.

    The compiler preserves extra template metadata:

    - source dependencies
    - subscription source locations
    - whether the string is plain, template-backed, or template-backed with dependencies

    Metadata propagation is intentionally direct:

    - template composition
    - assignment
    - return
    - direct argument passing
    - ordinary `String` parameters inserted into templates

    Ordinary string operations do not preserve reactivity in V1. They either snapshot once or produce a structured diagnostic according to the sink or operation contract.
]

#[section, $markdown:
    # Runtime sinks

    V1 has one live sink: top-level runtime HTML fragments in the HTML-JS builder.

    HTML-JS V1 mounts reactive template strings and rerenders the whole mount slot when any dependency source is invalidated.
    Plain string fragments keep the current insertion behavior.

    Unsupported or deferred sinks must not silently become live:

    - `io(...)` with direct subscription-bearing reactive templates is rejected until the IO/side-effect model is designed
    - `assert(...)` messages are string literals only, so reactive/runtime template messages are rejected
    - HTML-Wasm rejects reachable reactive runtime features before Wasm lowering
]

#[section, $markdown:
    # Backend ownership

    Reactive runtime helpers are backend artifacts, not user-visible closures or function values.

    The frontend records reactive source and template metadata. HIR carries backend-facing metadata and reachability facts. Backend feature validation decides which targets can lower a reachable reactive use.

    For the first HTML-JS implementation, whole-fragment rerendering is acceptable. Fine-grained text, attribute, style, keyed-loop, and nested-region updates are deferred optimizations.
]

#[section, $markdown:
    # Deferred follow-ups

    Future reactivity work includes:

    - reactive template control flow
    - field/path subscriptions
    - collection item subscriptions
    - expression dependency tracking
    - derived reactive values
    - reactive IO sinks
    - fine-grained DOM text, attribute, and style updates
    - nested reactive regions
    - keyed loop diffing and stable child identity
    - template-owned event/action/effect syntax
    - `$bind(...)` form binding helpers
    - typed component events/messages
    - HTML-Wasm reactive runtime support

    These are future UI and backend surfaces, not general function-value features.
]

#[section, $markdown:
    # Outside scope

    General closures, anonymous function values, generic function values, and higher-order polymorphism are outside the current Beanstalk language design scope.

    They may be reevaluated after Alpha only if the language philosophy explicitly changes.
]
