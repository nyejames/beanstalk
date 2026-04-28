import @html {center}
import @styles/docs {navbar, title, section, styled_table, header, data, codeblock, codesnippet, theme_head}

-- Dummy declarations so codesnippet templates referencing these symbols compile.
# count = 0
# snapshot = 0
# reactive = 0
# state = 0

# page_title = "Reactivity in Beanstalk Templates"
# page_description = "Design draft for explicit, place-based reactivity in Beanstalk templates using the state access mode."
# page_head = theme_head

#[navbar]
#[title, center: REACTIVITY IN BEANSTALK TEMPLATES]

#[section, $markdown:
    **Design draft:** This page is a future-facing design document for language and compiler planning.
    It is not final user-facing documentation, and the syntax, semantics, and implementation details may change before this feature is implemented.

    This document describes the design and semantics of **reactivity in Beanstalk templates**.

    Reactivity in Beanstalk is **explicit**, **place-based**, and built on the existing access-mode model of the language.

    A value does not become reactive when declared.
    A mutable place becomes reactive only when it is consumed through [codesnippet:state].
]

#[section, $markdown:
    # Design goals

    Reactivity in Beanstalk is designed to:

    - Be explicit and predictable
    - Keep reactive intent at the point of consumption
    - Reuse existing language concepts: places, mutability, access modes, and borrowing
    - Avoid declaration-level reactive markers
    - Avoid implicit reactive variables
    - Enable fine-grained node-level updates without virtual DOM diffing
    - Integrate cleanly with Beanstalk's GC-first memory model
    - Keep templates read-only

    Reactivity is not a separate framework layered on top of the language.
    It is a consequence of explicitly observing mutation.
]

#[section, $markdown:
    # Access modes

    Beanstalk values are read by default.

    There are three explicit access modes:

    - [codesnippet:copy place] creates an independent copied value
    - [codesnippet:~place] requests mutable/exclusive access
    - [codesnippet:state place] creates a reactive read subscription

    These access modes are mutually exclusive.

    [codeblock, $code("bst"):
        value          -- shared read
        copy value     -- explicit copy
        ~value         -- mutable/exclusive access
        state value    -- reactive read subscription
    ]

    Invalid combinations:

    [codeblock, $code("bst"):
        copy state value
        state copy value
        state ~value
        ~state value
    ]

    [codesnippet:state] does not copy a value, move a value, or grant mutable access.

    It creates a read-only subscription to a stable mutable place.
]

#[section, $markdown:
    # Core concept

    A template may capture values in two important ways:

    - **Snapshot capture:** the value is read once
    - **State capture:** the template subscribes to a mutable place and updates when it changes

    [codeblock, $code("bst"):
        count ~= 0

        snapshot = [count]
        reactive = [state count]
    ]

    [codesnippet, "[count]"] reads the current value of [codesnippet:count].

    [codesnippet, "[state count]"] subscribes to the mutable place [codesnippet:count].

    If [codesnippet:count] changes later, [codesnippet:snapshot] does not update, but [codesnippet:reactive] does.
]

#[section, $markdown:
    # Snapshot capture

    [codeblock, $code("bst"):
        [count]
    ]

    Snapshot capture:

    - Reads the current value of [codesnippet:count]
    - Creates no subscription
    - Does not promote [codesnippet:count] to reactive storage
    - Does not update if [codesnippet:count] changes later

    This is equivalent to reading a value normally in Beanstalk code.
]

#[section, $markdown:
    # State capture

    [codeblock, $code("bst"):
        [state count]
    ]

    State capture:

    - Captures a stable mutable place, not a copied value
    - Registers a dependency between the template binding and [codesnippet:count]
    - Updates the rendered output when [codesnippet:count] is mutated
    - Does not grant mutation permission to the template
    - Promotes [codesnippet:count] to reactive storage if needed

    State capture is only valid for mutable places.

    [codeblock, $code("bst"):
        count ~= 0
        view = [state count] -- valid
    ]

    [codeblock, $code("bst"):
        count = 0
        view = [state count] -- invalid
    ]

    The second example is invalid because immutable values cannot change, so subscribing to them is meaningless.
]

#[section, $markdown:
    # Example

    [codeblock, $code("bst"):
        @html/Basic

        count ~= 0

        increment_count |count ~Int|:
            count += 1
        ;

        [Basic.section:
            Current Count: [state count]
            Starting Count: [count]

            [Basic.button, Basic.onclick(
                ||: increment_count(~count)
            ):
                click me to increase the count!
            ]
        ]
    ]

    In this example:

    - [codesnippet:Current Count] is reactive and updates on every click
    - [codesnippet:Starting Count] is a snapshot and never changes
    - The callback mutates [codesnippet:count] through explicit mutable access
    - Mutating [codesnippet:count] invalidates render bindings subscribed with [codesnippet:state count]

    The callback is not reactive by itself.
    It is ordinary Beanstalk code that mutates a value.
    Reactivity occurs because a template binding explicitly subscribed to that value.
]

#[section, $markdown:
    # What is being captured

    A state capture does not capture:

    - A copied value
    - A string output
    - A computed expression
    - A mutable borrow

    It captures:

    - A stable place identity
    - A read-only reactive subscription to that place

    This aligns reactivity with Beanstalk's place-based compiler model.

    The template never owns the mutable access.
    It only observes changes.
]

#[section, $markdown:
    # Valid state sources

    Initially, [codesnippet:state] should be limited to simple stable places.

    Valid:

    [codeblock, $code("bst"):
        count ~= 0
        view = [state count]
    ]

    Invalid:

    [codeblock, $code("bst"):
        view = [state 1]
        view = [state get_count()]
        view = [state [:Hello]]
        view = [state copy count]
    ]

    [codesnippet:state] requires a stable source place.
    Literals, temporary values, copied values, and computed expressions do not have stable mutable identity.

    Future versions may support field paths and collection paths:

    [codeblock, $code("bst"):
        [state user.name]
        [state items.get(0)!]
    ]

    These should be added only after the basic place model is stable.
]

#[section, $markdown:
    # Function boundaries

    Reactive state must be explicit at both the call site and the function signature.

    A function that wants to observe a parameter reactively must declare that parameter with [codesnippet:state].

    [codeblock, $code("bst"):
        counter_view |count state Int| -> String:
            return [state count]
        ;
    ]

    The caller must also pass the argument using [codesnippet:state].

    [codeblock, $code("bst"):
        count ~= 0
        view = counter_view(state count)
    ]

    This is invalid:

    [codeblock, $code("bst"):
        view = counter_view(count)
    ]

    The callee requested a state-capable parameter, but the caller only provided a shared read.

    This is also invalid:

    [codeblock, $code("bst"):
        counter_view |count Int| -> String:
            return [state count]
        ;
    ]

    A normal parameter is not state-capable.
    The function signature must explicitly request state access.
]

#[section, $markdown:
    # State parameters are read-only

    A [codesnippet:state] parameter can be observed reactively, but it cannot be mutated.

    [codeblock, $code("bst"):
        counter_view |count state Int| -> String:
            return [state count]
        ;
    ]

    Mutation still requires mutable/exclusive access:

    [codeblock, $code("bst"):
        increment_count |count ~Int|:
            count += 1
        ;
    ]

    These are different capabilities:

    - [codesnippet:count state Int] is a reactive read subscription
    - [codesnippet:count ~Int] is mutable/exclusive access

    A parameter cannot be both [codesnippet:state] and [codesnippet:~].

    Invalid:

    [codeblock, $code("bst"):
        bad |count state ~Int|:
            count += 1
        ;
    ]

    State access and mutable access are mutually exclusive.
]

#[section, $markdown:
    # State is not a type

    [codesnippet:state] is an access mode, not a user-facing wrapper type.

    [codeblock, $code("bst"):
        counter_view |count state Int| -> String:
    ]

    This does not mean [codesnippet:count] has a type like [codesnippet:State<Int>].

    It means:

    - [codesnippet:count] has value type [codesnippet:Int]
    - The parameter receives reactive subscription-capable access
    - The function may use [codesnippet:state count] inside templates or other state-consuming expressions

    The compiler and backend may lower this to a reactive cell handle internally, but that representation does not leak into the source language.
]

#[section, $markdown:
    # Reactivity and variable declarations

    Variables are not marked reactive when declared.

    [codeblock, $code("bst"):
        count ~= 0
    ]

    There is no declaration-level [codesnippet:state] syntax.

    A mutable place becomes reactive only when it is consumed through [codesnippet:state].

    [codeblock, $code("bst"):
        count ~= 0
        view = [state count]
    ]

    This avoids:

    - Global reactivity flags
    - Declaration-level reactive annotations
    - Unnecessary runtime overhead
    - Multiple ways to create reactive state

    The point of consumption decides whether a place is observed reactively.
]

#[section, $markdown:
    # Assignment semantics

    When a mutable place has been promoted to reactive storage, assignment updates the same stable source.

    [codeblock, $code("bst"):
        message ~= [:Hello]
        view = [state message]

        message = [:Goodbye]
    ]

    The assignment does not break the subscription.

    It means:

    - The same source place still exists
    - The value stored in that source changes
    - Subscribers to [codesnippet:state message] are invalidated
    - Render bindings depending on [codesnippet:message] update

    A reactive source has stable identity.
    Assignment updates the source value, not the subscription target.
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
    - Reactivity occurs as a result of mutation to a place observed with [codesnippet:state]

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

    State captures may bind to backend render targets such as:

    - Text nodes
    - Attributes
    - Styles
    - Future backend-defined node targets

    The HTML backend should lower reactive templates into a node-level render plan.

    There is no virtual DOM diffing step.
    The backend knows which render targets depend on which state sources and can update those targets directly.

    Example:

    [codeblock, $code("bst"):
        message ~= [:Hello]

        [Basic.section:
            [state message]
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

    should produce one update flush, not three separate render passes.

    The scheduler is backend-owned.
    It is not a user-facing global state system.

    The user-facing rule remains:

    > Mutating a place updates all render bindings that explicitly subscribed to that place with [codesnippet:state].
]

#[section, $markdown:
    # Render plans

    Reactive templates should not be implemented as ordinary string rerendering.

    The compiler should produce a render plan that distinguishes:

    - Static text
    - Snapshot expressions
    - State expressions
    - Child templates
    - Attributes
    - Styles
    - Backend-specific render targets

    A simplified internal model:

    [codeblock, $code("rust"):
        enum TemplateFragment {
            StaticText(StringId),
            SnapshotExpression(ExpressionId),
            StateExpression(StateCaptureId),
            ChildTemplate(TemplatePlanId),
        }

        struct StateCapture {
            place: PlaceId,
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

    ## Phase 1: State access parsing and validation

    Add [codesnippet:state] as an access mode.

    Validate that:

    - [codesnippet:state] applies only to stable places
    - [codesnippet:state] cannot combine with [codesnippet:copy] or [codesnippet:~]
    - [codesnippet:state] requires a mutable source place
    - [codesnippet:state] arguments match [codesnippet:state] parameters
    - normal parameters cannot be consumed with [codesnippet:state]
    - templates can only observe [codesnippet:state] values, never mutate them

    ## Phase 2: Runtime reactive cells

    Promote state-consumed places to reactive storage.

    In GC-backed builds, this can be represented as a heap-managed reactive cell.

    Assignment to a promoted place becomes a cell update plus invalidation.

    ## Phase 3: Reactive template plans

    Extend template render plans to preserve state captures.

    The template system should distinguish:

    - [codesnippet, "[count]"] as snapshot capture
    - [codesnippet, "[state count]"] as reactive state capture

    ## Phase 4: HTML backend node updates

    Lower state captures to node-level update bindings.

    Start with simple targets:

    - Text node contents
    - Attribute values
    - Style values

    ## Phase 5: Coarser fallbacks

    Where direct node updates are too complex, the backend may rerender a bounded template region.

    This should be a fallback, not the core model.
]

#[section, $markdown:
    # Lifetime and scope constraints

    State subscriptions are not mutable borrows and do not hold exclusive access.

    When a mutable place is consumed through [codesnippet:state], the compiler may promote that place to stable heap-managed storage.
    This allows templates, callbacks, and returned views to safely retain subscriptions where the backend supports it.

    In GC-backed builds, reactive cells can be kept alive by normal GC reachability.

    In future ownership-optimized Wasm builds, the compiler must ensure that reactive cells are not freed while live subscriptions exist.

    The compiler should reject state subscriptions to places that cannot be safely promoted or represented by the target backend.
]

#[section, $markdown:
    # Relationship to borrowing

    State access is separate from mutable borrowing.

    [codeblock, $code("bst"):
        count ~= 0

        view = [state count]
        increment_count(~count)
    ]

    The template subscription does not block later mutable access to [codesnippet:count], because it is not a mutable borrow.

    The mutation invalidates subscribers.

    This is the intended interaction:

    - [codesnippet:state count] observes changes
    - [codesnippet:~count] mutates the place
    - mutation triggers reactive updates

    The borrow checker should not model [codesnippet:state count] as an active mutable borrow.
    It should model it as a subscription to a stable place.
]

#[section, $markdown:
    # Structs and fields

    Field-level state tracking should be deferred.

    Initial reactivity should focus on simple local places.

    Later, this may become valid:

    [codeblock, $code("bst"):
        user ~= User(name = "Nye", age = 30)
        view = [state user.name]
    ]

    The first version can use coarse invalidation:

    - Any mutation to [codesnippet:user] invalidates state captures of [codesnippet:user]
    - Field-specific invalidation can be added later

    Fine-grained path tracking should not block the first implementation.
]

#[section, $markdown:
    # Collections

    Reactive collections should also be deferred beyond the basic model.

    A conservative first version may treat a collection as one reactive source.

    [codeblock, $code("bst"):
        items ~{String} = {}

        view = [state items]
    ]

    Any mutation to [codesnippet:items] invalidates the whole binding.

    Later versions may support:

    - Item-level subscriptions
    - Collection-shape subscriptions
    - Keyed loops
    - Stable child template identity

    Keyed loop syntax and list diffing should be designed separately.
]

#[section, $markdown:
    # Mutation during render

    Templates are read-only.

    A template may observe state, but it may not mutate state during rendering.

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

    - Explicit: written with [codesnippet:state]
    - Place-based: subscriptions target stable mutable places
    - Access-mode based: [codesnippet:state], [codesnippet:copy], and [codesnippet:~] are mutually exclusive
    - Mutation-driven: updates occur when subscribed places are mutated
    - Read-only from templates: templates can observe but never mutate
    - Not declaration-based: variables are not marked reactive where they are declared
    - Backend-owned: render plans decide how updates are applied efficiently

    [codeblock, $code("bst"):
        count ~= 0

        snapshot = [count]
        reactive = [state count]

        count = 1
    ]

    [codesnippet:snapshot] sees the original value.

    [codesnippet:reactive] observes the place and updates.

    The core rule is:

    > Mutating a place updates all render bindings that explicitly subscribed to that place with [codesnippet:state].
]