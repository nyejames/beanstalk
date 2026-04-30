import @html {center}
import @styles/docs {navbar, title, section, styled_table, header, data, codeblock, codesnippet, theme_head}

# page_title = "Reactivity in Beanstalk Templates"
# page_description = "Design draft for explicit, place-based reactivity in Beanstalk templates using reactive type access."
# page_head = theme_head

#[navbar]
#[title, center: REACTIVITY IN BEANSTALK TEMPLATES]

#[section, $markdown:
    **Design draft:** This page is a future-facing design document for language and compiler planning.
    It is not final user-facing documentation, and the syntax, semantics, and implementation details may change before this feature is implemented.

    This document describes the design and semantics of **reactivity in Beanstalk templates**.

    Reactivity in Beanstalk is **explicit**, **place-based**, and tied to compiler-managed template rendering.

    Reactive values are declared with **reactive type access**:

    [codeblock, $code("bst"):
        count $Int = 0
        message $String = [:Hello]
    ]

    Templates subscribe to reactive values with [codesnippet:~] inside template captures:

    [codeblock, $code("bst"):
        [~count]
    ]

    Regular code can still read reactive values normally:

    [codeblock, $code("bst"):
        doubled = count * 2
    ]
]

#[section, $markdown:
    # Design goals

    Reactivity in Beanstalk is designed to:

    - Be explicit and predictable
    - Keep reactive storage visible at declaration sites
    - Keep subscriptions visible inside templates
    - Reuse existing language concepts: types, places, mutability, and template captures
    - Avoid hidden reactive promotion
    - Avoid virtual DOM diffing
    - Enable fine-grained node-level updates
    - Integrate cleanly with Beanstalk's GC-first memory model
    - Keep templates read-only

    Reactivity is not a separate framework layered on top of the language.

    It is compiler-owned rendering behavior built around stable reactive places.
]

#[section, $markdown:
    # Core idea

    Reactive values are declared by prefixing the value type with [codesnippet:$].

    [codeblock, $code("bst"):
        count $Int = 0
    ]

    This means:

    - [codesnippet:count] has value type [codesnippet:Int]
    - [codesnippet:count] is stored as a reactive source
    - [codesnippet:count] has stable identity
    - templates can subscribe to [codesnippet:count] with [codesnippet, "[~count]"]
    - assignment to [codesnippet:count] invalidates subscribers

    Regular code does not need special syntax to read it:

    [codeblock, $code("bst"):
        count $Int = 0

        next = count + 1
        label = [:Count: [count]]
    ]

    In a template, [codesnippet, "[count]"] is a snapshot read.

    In a template, [codesnippet, "[~count]"] is a reactive subscription.
]

#[section, $markdown:
    # Reactive type access

    Reactive type access uses [codesnippet:$Type].

    [codeblock, $code("bst"):
        count $Int = 0
        enabled $Bool = true
        message $String = [:Hello]
    ]

    It works with user-defined types too:

    [codeblock, $code("bst"):
        ReactiveData = |
            time Int
        |

        data $ReactiveData = ReactiveData(1)
    ]

    [codesnippet:$Type] does not create a new user-facing wrapper type.

    This:

    [codeblock, $code("bst"):
        count $Int = 0
    ]

    does not mean [codesnippet:count] has type [codesnippet:State<Int>].

    It means [codesnippet:count] is an [codesnippet:Int] stored in compiler-managed reactive storage.
]

#[section, $markdown:
    # Inferred reactive declarations

    Reactive declarations can also infer their value type from the initializer.

    [codeblock, $code("bst"):
        count $= 0
        message $= [:Hello]

        ReactiveData = |
            time Int
        |

        data $= ReactiveData(1)
    ]

    [codesnippet:$=] is an inferred reactive declaration.

    It is declaration syntax, not assignment syntax.

    After declaration, reassignment uses normal assignment:

    [codeblock, $code("bst"):
        count $= 0

        count = 1
        count += 1
    ]

    This is invalid:

    [codeblock, $code("bst"):
        count $= 0

        count $= 1 -- invalid
    ]

    [codesnippet:$=] is only used when declaring a new reactive value.
]

#[section, $markdown:
    # Snapshot vs subscription

    Templates can read a reactive value in two ways.

    [codeblock, $code("bst"):
        count $Int = 0

        snapshot = [count]
        reactive = [~count]
    ]

    [codesnippet, "[count]"] reads the current value.

    [codesnippet, "[~count]"] subscribes to the reactive source.

    If [codesnippet:count] changes later, [codesnippet:snapshot] does not update, but [codesnippet:reactive] does.

    This makes snapshot and reactive rendering explicit at the place where the template uses the value.
]

#[section, $markdown:
    # Template subscription

    Inside templates, [codesnippet:~reactive_value] subscribes to a reactive source.

    [codeblock, $code("bst"):
        count $Int = 0

        [~count]
    ]

    This is only valid for reactive values.

    This is invalid:

    [codeblock, $code("bst"):
        count ~= 0

        [~count]
    ]

    [codesnippet:count] is mutable, but it is not reactive.

    The compiler should produce a clear error:

    [codeblock, $code("text"):
        Cannot subscribe to 'count' because it is not reactive.
        Use '[count]' for a snapshot, or declare it with '$Int' or '$='.
    ]
]

#[section, $markdown:
    # Meaning of ~

    [codesnippet:~] keeps its normal meaning outside templates.

    [codeblock, $code("bst"):
        increment |count ~Int|:
            count += 1
        ;

        count $Int = 0
        increment(~count)
    ]

    Outside templates, [codesnippet:~count] requests mutable/exclusive access.

    Inside templates, [codesnippet, "[~count]"] subscribes to a reactive value.

    Templates still cannot mutate values.

    The two meanings are context-specific:

    [styled_table:
        [header: [: Syntax] [: Context] [: Meaning] ]

        [data:
            [: [codesnippet:~count] ]
            [: Normal code / call argument ]
            [: Mutable/exclusive access ]
        ]

        [data:
            [: [codesnippet, "[~count]"] ]
            [: Template capture ]
            [: Reactive read subscription ]
        ]
    ]
]

#[section, $markdown:
    # Reactive declarations imply mutable storage

    Reactive declarations are mutation-capable by definition.

    [codeblock, $code("bst"):
        count $Int = 0

        count = 1
        count += 1
    ]

    This is deliberate.

    Reactivity exists to observe changes, so a reactive source must be able to change.

    These forms are invalid because they mix reactive storage syntax with mutable declaration syntax:

    [codeblock, $code("bst"):
        count $Int ~= 0
        count ~$Int = 0
        count $~Int = 0
    ]

    Use this instead:

    [codeblock, $code("bst"):
        count $Int = 0
    ]
]

#[section, $markdown:
    # Function boundaries

    Functions that need a reactive source use reactive type access in the parameter type.

    [codeblock, $code("bst"):
        counter_view |count $Int| -> String:
            return [:
                Current Count: [~count]
                Starting Count: [count]
            ]
        ;
    ]

    The caller passes the value normally:

    [codeblock, $code("bst"):
        count $Int = 0
        view = counter_view(count)
    ]

    The function signature requests a reactive source.

    The call site does not use [codesnippet:~], because [codesnippet:~] outside templates means mutable access.

    This is invalid:

    [codeblock, $code("bst"):
        count ~= 0
        view = counter_view(count)
    ]

    [codesnippet:counter_view] requires [codesnippet:$Int], but [codesnippet:count] is a normal mutable [codesnippet:Int].

    The compiler should produce a clear error:

    [codeblock, $code("text"):
        Function 'counter_view' requires reactive argument '$Int' for parameter 'count'.
        Declare the argument with '$Int' or '$=' if it should be reactive.
    ]
]

#[section, $markdown:
    # Reactive parameters are not mutable parameters

    A reactive parameter can be subscribed to, but it does not grant mutation permission by itself.

    [codeblock, $code("bst"):
        counter_view |count $Int| -> String:
            return [~count]
        ;
    ]

    Mutation still requires a mutable parameter:

    [codeblock, $code("bst"):
        increment |count ~Int|:
            count += 1
        ;
    ]

    These are different capabilities:

    - [codesnippet:count $Int] requires a reactive [codesnippet:Int] source
    - [codesnippet:count ~Int] requires mutable/exclusive access to an [codesnippet:Int]

    A parameter cannot combine [codesnippet:$] and [codesnippet:~].

    Invalid:

    [codeblock, $code("bst"):
        bad |count ~$Int|:
            count += 1
        ;

        also_bad |count $~Int|:
            count += 1
        ;
    ]
]

#[section, $markdown:
    # Full example

    [codeblock, $code("bst"):
        @html/Basic

        count $Int = 0

        increment_count |count ~Int|:
            count += 1
        ;

        counter_view |count $Int| -> String:
            return [:
                Current Count: [~count]
                Starting Count: [count]
            ]
        ;

        [Basic.section:
            [counter_view(count)]

            [Basic.button, Basic.onclick(
                ||: increment_count(~count)
            ):
                click me to increase the count!
            ]
        ]
    ]

    In this example:

    - [codesnippet:count $Int = 0] declares a reactive source
    - [codesnippet, "[~count]"] subscribes to that source
    - [codesnippet, "[count]"] snapshots its current value
    - [codesnippet:increment_count(~count)] mutates it
    - mutation invalidates template bindings subscribed to [codesnippet, "[count]"]

    The callback is not reactive by itself.

    It is ordinary Beanstalk code that mutates a value.
    Reactivity occurs because a template binding explicitly subscribed to a reactive source.
]

#[section, $markdown:
    # Compiler directive namespace

    [codesnippet:$] should be treated as a compiler-owned marker with different legal forms in different syntax positions.

    Lowercase names are template/style directives:

    [codeblock, $code("bst"):
        [$markdown:
            # Title
        ]
    ]

    PascalCase names after [codesnippet:$] in type position are reactive type access:

    [codeblock, $code("bst"):
        count $Int = 0
        data $ReactiveData = ReactiveData(1)
    ]

    [codesnippet:$=] is inferred reactive declaration syntax:

    [codeblock, $code("bst"):
        count $= 0
    ]

    This relies on Beanstalk's naming rules:

    - types use PascalCase
    - variables and functions use snake_case
    - template/style directives use lowercase names

    Because these naming conventions are enforced, [codesnippet:$markdown] and [codesnippet:$Int] cannot collide.
]

#[section, $markdown:
    # Invalid directive forms

    The parser should reject invalid [codesnippet:$] usage clearly.

    Invalid lowercase directive in type position:

    [codeblock, $code("bst"):
        count $markdown = 0
    ]

    Invalid PascalCase type directive in template directive position:

    [codeblock, $code("bst"):
        [$Int:
            content
        ]
    ]

    Invalid reactive type without initializer:

    [codeblock, $code("bst"):
        count $Int
    ]

    Invalid inferred reactive declaration without initializer:

    [codeblock, $code("bst"):
        count $=
    ]

    Invalid reactive type after the initializer:

    [codeblock, $code("bst"):
        count Int = $0
        count Int = 0 $Int
    ]

    Invalid subscription to a non-reactive value:

    [codeblock, $code("bst"):
        count ~= 0
        [~count]
    ]

    Invalid subscription to a computed expression:

    [codeblock, $code("bst"):
        [~(count + 1)]
        [~get_count()]
        [~[:Hello]]
    ]

    Reactive subscriptions require a named reactive source with stable identity.
]

#[section, $markdown:
    # What is being captured

    A reactive template capture does not capture:

    - A copied value
    - A string output
    - A computed expression
    - A mutable borrow

    It captures:

    - A stable reactive source identity
    - A read-only subscription to that source

    The template never owns mutable access.

    It only observes changes.
]

#[section, $markdown:
    # Valid reactive sources

    Initially, template subscriptions should be limited to simple reactive places.

    Valid:

    [codeblock, $code("bst"):
        count $Int = 0
        view = [~count]
    ]

    Invalid:

    [codeblock, $code("bst"):
        count ~= 0

        view = [~count]
        view = [~1]
        view = [~get_count()]
        view = [~[:Hello]]
    ]

    Future versions may support field paths and collection paths:

    [codeblock, $code("bst"):
        [~user.name]
        [~items.get(0)!]
    ]

    These should be added only after the basic reactive source model is stable.
]

#[section, $markdown:
    # Assignment semantics

    Assignment to a reactive value updates the same stable source.

    [codeblock, $code("bst"):
        message $String = [:Hello]
        view = [~message]

        message = [:Goodbye]
    ]

    The assignment does not break the subscription.

    It means:

    - The same reactive source still exists
    - The value stored in that source changes
    - Subscribers to [codesnippet, "[message]"] are invalidated
    - Render bindings subscribed with [codesnippet, "[~message]"] update

    A reactive source has stable identity.

    Assignment updates the source value, not the subscription target.
]

#[section, $markdown:
    # Regular reads

    Reactive values can be read as ordinary values.

    [codeblock, $code("bst"):
        count $Int = 0

        doubled = count * 2
        label = [:Count: [count]]
    ]

    This does not create a subscription.

    A regular read sees the current value.

    A template snapshot captures the current rendered value.

    Only [codesnippet, "[~count]"] subscribes.
]

#[section, $markdown:
    # Passing reactive values to ordinary functions

    A reactive value can be passed to ordinary parameters as a normal value.

    [codeblock, $code("bst"):
        double |value Int| -> Int:
            return value * 2
        ;

        count $Int = 0
        result = double(count)
    ]

    This is a normal read.

    It does not subscribe [codesnippet:double] to [codesnippet:count].

    A function must explicitly request reactive access with [codesnippet:$Int] if it needs to subscribe:

    [codeblock, $code("bst"):
        counter_view |count $Int| -> String:
            return [~count]
        ;
    ]
]

#[section, $markdown:
    # Callbacks and events

    Callbacks in Beanstalk templates are ordinary closures with normal access rules.

    [codeblock, $code("bst"):
        Basic.onclick(||:
            increment_count(~count)
        )
    ]

    Key properties:

    - Closures capture values using normal language rules
    - Mutable access must be explicit
    - Callbacks do not create reactivity by themselves
    - Reactivity occurs as a result of mutation to a reactive source observed with [codesnippet, "[~value]"]

    This should remain true even when higher-level event helpers are added.
]

#[section, $markdown:
    # Declarative event syntax

    Higher-level declarative event syntax may be introduced later as sugar:

    [codeblock, $code("bst"):
        [Basic.button:
            on click:
                increment_count(~count)
            ;
            click me
        ]
    ]

    This should lower to ordinary callback passing.

    It must not change the underlying access rules.
]

#[section, $markdown:
    # Reactive sinks

    Reactive captures may bind to backend render targets such as:

    - Text nodes
    - Attributes
    - Styles
    - Future backend-defined node targets

    The HTML backend should lower reactive templates into a node-level render plan.

    There is no virtual DOM diffing step.

    The backend knows which render targets depend on which reactive sources and can update those targets directly.

    Example:

    [codeblock, $code("bst"):
        message $String = [:Hello]

        [Basic.section:
            [~message]
        ]
    ]

    This can lower to a text-node binding whose contents are updated whenever [codesnippet:message] changes.
]

#[section, $markdown:
    # Runtime update scheduling

    Reactive updates should be scheduled and batched by the backend runtime.

    Multiple mutations in one synchronous block should not cause multiple independent DOM updates.

    Conceptually:

    [codeblock, $code("bst"):
        name = "Nye"
        count = 1
        enabled = true
    ]

    should produce one update flush, not three separate render passes, when those assignments affect reactive sources.

    The scheduler is backend-owned.

    It is not a user-facing global state system.

    The user-facing rule remains:

    > Mutating a reactive source updates all render bindings that explicitly subscribed to that source with [codesnippet:~] inside templates.
]

#[section, $markdown:
    # Render plans

    Reactive templates should not be implemented as ordinary string rerendering.

    The compiler should produce a render plan that distinguishes:

    - Static text
    - Snapshot expressions
    - Reactive subscriptions
    - Child templates
    - Attributes
    - Styles
    - Backend-specific render targets

    A simplified internal model:

    [codeblock, $code("rust"):
        enum TemplateFragment {
            StaticText(StringId),
            SnapshotExpression(ExpressionId),
            ReactiveExpression(ReactiveCaptureId),
            ChildTemplate(TemplatePlanId),
        }

        struct ReactiveCapture {
            source: ReactiveSourceId,
            value_type: DataType,
            location: SourceLocation,
        }
    ]

    The frontend should preserve semantic reactivity in the template representation.

    The backend decides how to lower it.

    For HTML, this should eventually become node-level DOM update code rather than string replacement where practical.
]

#[section, $markdown:
    # Initial implementation strategy

    The first implementation should be conservative.

    ## Phase 1: Reactive type syntax

    Add parsing and validation for:

    - explicit reactive declarations: [codesnippet:count $Int = 0]
    - inferred reactive declarations: [codesnippet:count $= 0]
    - reactive parameters: [codesnippet:count $Int]
    - template subscriptions: [codesnippet, "[~count]"]

    Reject invalid combinations clearly.

    ## Phase 2: Reactive source validation

    Validate that:

    - [codesnippet:$Type] uses a real value type
    - [codesnippet:$=] has an initializer
    - reactive declarations are not combined with [codesnippet:~=]
    - reactive parameters are not combined with [codesnippet:~]
    - [codesnippet, "[~value]"] only targets reactive sources
    - [codesnippet, "[~value]"] is only used with stable places
    - templates can observe reactive values, but never mutate them

    ## Phase 3: Runtime reactive cells

    Lower reactive declarations to stable reactive storage.

    In GC-backed builds, this can be represented as a heap-managed reactive cell.

    Assignment to a reactive value becomes a cell update plus invalidation.

    ## Phase 4: Reactive template plans

    Extend template render plans to preserve reactive captures.

    The template system should distinguish:

    - [codesnippet, "[count]"] as snapshot capture
    - [codesnippet, "[~count]"] as reactive subscription

    ## Phase 5: HTML backend node updates

    Lower reactive captures to node-level update bindings.

    Start with simple targets:

    - Text node contents
    - Attribute values
    - Style values

    ## Phase 6: Coarser fallbacks

    Where direct node updates are too complex, the backend may rerender a bounded template region.

    This should be a fallback, not the core model.
]

#[section, $markdown:
    # Lifetime and scope constraints

    Reactive subscriptions are not mutable borrows and do not hold exclusive access.

    Reactive values are already declared as stable reactive sources.

    In GC-backed builds, reactive cells can be kept alive by normal GC reachability.

    In future ownership-optimized Wasm builds, the compiler must ensure that reactive cells are not freed while live subscriptions exist.

    The compiler should reject reactive subscriptions to places that cannot be safely represented by the target backend.
]

#[section, $markdown:
    # Relationship to borrowing

    Reactive template subscription is separate from mutable borrowing.

    [codeblock, $code("bst"):
        count $Int = 0

        view = [~count]
        increment_count(~count)
    ]

    The template subscription does not block later mutable access to [codesnippet:count], because it is not a mutable borrow.

    The mutation invalidates subscribers.

    This is the intended interaction:

    - [codesnippet, "[~count]"] observes changes in templates
    - [codesnippet, "~count"] mutates the place in normal code
    - mutation triggers reactive updates

    The borrow checker should not model [codesnippet, "[~count]"] as an active mutable borrow.

    It should model it as a subscription to a stable reactive source.
]

#[section, $markdown:
    # Structs and fields

    Field-level reactive tracking should be deferred.

    Initial reactivity should focus on simple named reactive sources.

    Later, this may become valid:

    [codeblock, $code("bst"):
        User = |
            name String
            age Int
        |

        user $User = User("Nye", 30)

        view = [~user.name]
    ]

    The first version can use coarse invalidation:

    - any mutation to [codesnippet:user] invalidates subscriptions to [codesnippet:user]
    - field-specific invalidation can be added later

    Fine-grained path tracking should not block the first implementation.
]

#[section, $markdown:
    # Collections

    Reactive collections should also be deferred beyond the basic model.

    A conservative first version may treat a collection as one reactive source.

    [codeblock, $code("bst"):
        items ${String} = {}

        view = [~items]
    ]

    Any mutation to [codesnippet:items] invalidates the whole binding.

    Later versions may support:

    - Item-level subscriptions
    - Collection-shape subscriptions
    - Keyed loops
    - Stable child template identity

    Keyed loop syntax and list diffing should be designed separately.

    The exact syntax for reactive collection types may need special parser treatment because collection types use braces.
]

#[section, $markdown:
    # Mutation during render

    Templates are read-only.

    A template may observe reactive values, but it may not mutate values during rendering.

    This avoids:

    - Render loops
    - Re-entrant update bugs
    - Ordering ambiguity
    - Hidden side effects in templates

    Rendering should remain a pure description of output from current values.
]

#[section, $markdown:
    # Summary

    Reactivity in Beanstalk is:

    - Explicit: reactive sources are declared with [codesnippet: $Type] or [codesnippet:$=]
    - Template-specific: subscriptions are written as [codesnippet, "[~value]"] inside templates
    - Place-based: subscriptions target stable reactive sources
    - Mutation-driven: updates occur when subscribed reactive sources are mutated
    - Read-only from templates: templates can observe but never mutate
    - Compiler-owned: [codesnippet:$] marks compiler-managed behavior
    - Backend-owned: render plans decide how updates are applied efficiently

    [codeblock, $code("bst"):
        count $Int = 0

        snapshot = [count]
        reactive = [~count]

        count = 1
    ]

    [codesnippet:snapshot] sees the original value.

    [codesnippet:reactive] observes the reactive source and updates.

    The core rule is:

    > Mutating a reactive source updates all render bindings that explicitly subscribed to that source with [codesnippet:~] inside templates.
]