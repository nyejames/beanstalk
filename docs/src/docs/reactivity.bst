@(libs/html/Basic)
@(styles/Docs)
@(./components)

[Basic.page:

[Docs.navbar]

[Docs.header, Basic.center:
    [Basic.title: REACTIVITY]
]

This document describes the design and semantics of **reactivity in Beanstalk templates**.

Reactivity in Beanstalk is **explicit**, **place-based**, and built directly on the existing mutability model of the language. There are no implicit reactive variables, no hidden schedulers, and no global UI state. A value only becomes reactive when it is *captured reactively* by a template.

---

# Design Goals

Reactivity in Beanstalk is designed to:

* Be explicit and predictable
* Reuse existing language concepts (mutability, places, borrowing)
* Avoid implicit magic or compiler rewriting
* Enable fine-grained updates without virtual DOM diffing
* Integrate cleanly with Beanstalk's memory and ownership model

Reactivity is not a separate system layered on top of the language. It is a **consequence of observable mutation**.

---

# Core Concept

A template may capture values in one of two ways:

* **Snapshot capture**: the value is read once at template creation
* **Reactive capture**: the template observes a mutable place and updates when it changes

The distinction is made explicitly using the mutability symbol `~`.

---

# Snapshot vs Reactive Capture

## Snapshot Capture

```beanstalk
[count]
```

* Captures the *current value* of `count`
* No future relationship exists
* The template output does not change if `count` changes later

This is equivalent to reading a value normally in Beanstalk code.

---

## Reactive Capture

```beanstalk
[~count]
```

* Captures a **mutable place**, not a value
* Registers a dependency between the template and `count`
* Updates the rendered output whenever `count` is mutated

Reactive capture is only possible for values that can be mutably accessed.

---

# Example

```beanstalk
@(html/Basic)

count ~= 0

increment_count |count ~int|:
    count += 1
;

[Basic.section:
    Current Count: [~count]
    Starting Count: [count]

    [Basic.button, Basic.onclick(
        ||: increment_count(count)
    ):
        click me to increase the count!
    ]
]
```

In this example:

* `Current Count` is reactive and updates on every click
* `Starting Count` is a snapshot and never changes
* The callback mutates `count`, triggering reactive updates

---

# What Is Being Captured

A reactive template capture does **not** capture:

* A copied value
* A string
* A computed expression

Instead, it captures:

* A *place identity* (the same abstraction used by the compiler)
* Along with a marker indicating reactive observation

This aligns with Beanstalk's place-based memory model and borrow semantics.

---

# Reactive Sinks

Reactive captures may bind to the following template outputs:

* Text nodes
* Attributes
* Styles

These bindings update directly when the underlying place changes. There is no DOM diffing step.

---

# Callbacks and Events

Callbacks in Beanstalk templates are not special language features. They are ordinary closures with explicit mutation.

```beanstalk
Basic.onclick(||:
    increment_count(count)
)
```

Key properties:

* Closures capture places using normal language rules
* Mutable access must be explicit
* Callbacks do not interact with reactivity directly

Reactivity occurs *as a result* of mutation, not because the callback is special.

---

# Declarative Event Syntax (Future)

Higher-level declarative syntax may be introduced later as sugar:

```beanstalk
[Basic.button:
    on click:
        increment_count(count)
    ;
    click me
]
```

This is purely syntactic sugar for passing a closure. It does not change the underlying semantics.

---

# Reactivity and Variable Declarations

Variables do **not** need to be marked as reactive when declared.

```beanstalk
count ~= 0
```

A variable becomes reactive **only when captured reactively by a template**.

This avoids:

* Global reactivity flags
* Implicit compiler rewriting
* Unnecessary runtime overhead

---

# Lifetime and Scope Constraints

Reactive template captures must obey scope and lifetime rules.

* A template may not outlive the mutable places it observes
* Reactive captures mirror borrow semantics
* Conservative rules may be enforced initially

These constraints integrate cleanly with Beanstalk's GC-first execution model.

---

# Summary

Reactivity in Beanstalk is:

* Explicit (`~`)
* Place-based
* Mutation-driven
* Fine-grained
* Predictable

Templates do not re-run. DOM nodes do not diff. State does not magically update UI.

Instead:

> Mutating a place updates all template bindings that explicitly observe that place.

This keeps reactivity small, understandable, and aligned with the rest of the language design.

[Docs.footer]

]