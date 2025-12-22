import @libs/html/Basic
import @styles/Docs
import @./components
[Basic.page:

[Docs.navbar]
[Docs.header, Basic.center:
    [Basic.title: Aynsc and Coroutines]
]

## Coroutines and Async in Beanstalk (Design Overview)

Beanstalk approaches async and concurrency by making **control flow explicit** and **communication safe by default**. 
Instead of hiding async behind keywords like `async` / `await` or complex runtime abstractions, 
Beanstalk uses a small set of primitives that make data flow, suspension, and concurrency visible in the syntax.

At a high level:

* Coroutines are lightweight, cooperative tasks
* They communicate using typed channels
* All concurrency is structured and scoped
* The compiler statically enforces safety (no data races, no dangling references)

This keeps async code readable, predictable, and analyzable.

---

## Channels: communicating between coroutines

Coroutines do not share mutable state. Instead, they communicate by sending values through **channels**.

### Channel creation: `=>`

A channel is created using the `=>` operator, which produces two endpoints:

```beanstalk
in, out => Int
```

* `out` is the sending endpoint
* `in` is the receiving endpoint
* The channel carries values of type `Int`

You can think of this as creating a one-way pipe for values of a specific type.

---

### Sending values: `>>`

To send a value into a channel, use `>>`:

```beanstalk
value >> out
```

This transfers the value into the channel. Sending may **suspend** the current coroutine if the receiver is not ready.

Visually, the value flows to the right into the channel.

---
### Receiving values: `<<`

To receive a value from a channel, use `<<`:

```beanstalk
value << in
```

This waits until a value is available, then assigns it to `value`. Receiving may also **suspend** the current coroutine.

Visually, the value flows left out of the channel.

---

## Coroutines: running work concurrently

A coroutine is a function that can suspend and resume execution. Coroutines are started using `spawn` and always run inside a structured async scope.

Example coroutine functions:

```beanstalk
producer |out ~Channel[Int]|:
    loop i in 0 to 10:
        i >> out
        yield
    ;
;

consumer |in ~Channel[Int]|:
    loop:
        value << in
        io(value)
    ;
;
```

Here:
* `producer` sends integers into the channel
* `consumer` receives and prints them
* Neither shares memory directly
* All interaction goes through the channel

---

## Suspending execution: `yield`

`yield` explicitly suspends the current coroutine and allows others to run:

```beanstalk
yield
```

This is a cooperative pause point. It does not block the program; it simply tells the scheduler:

> “I’m done for now — let something else run.”

More complex example:

```beanstalk
loop i in 0 to 1_000_000:
    i >> out

    if i % 128 == 0:
        yield
    ;
;
```

Here, `yield` prevents one coroutine from starving others by periodically giving up control.

Importantly, suspension is **always visible** in the code. There are no hidden pause points.

---

## Structured concurrency: `#async` scope

All coroutines in Beanstalk run inside a special async scope, introduced with a compiler directive:

```beanstalk
#async:
    ...
;
```

An `#async` scope means:

* Coroutines may be spawned inside
* Suspension (`yield`, channel send/receive) is allowed
* All spawned coroutines must complete before the scope exits
* No background or detached tasks exist

Example putting everything together:

```beanstalk
#async:
    in, out => Int

    spawn producer(out)
    spawn consumer(in)

    loop:
        yield
    ;
;
```

This makes concurrency **structured** and **bounded**. You always know where async work starts and where it must finish.

---

## Why this solves async cleanly

This model avoids many common async pitfalls:

* No implicit blocking
* No hidden scheduling
* No shared mutable state
* No “viral” async types
* No background tasks that outlive their data

Instead, async in Beanstalk is:

* Explicit in syntax
* Local in reasoning
* Enforced by the compiler
* Easy to visualize as data flow

For newcomers, this means async code looks like ordinary code — just with arrows showing where values move and where execution may pause.

---

## Current status and future considerations

⚠️ **This design is not yet implemented.**

Beanstalk’s coroutine and async model is still in the design phase and depends on several external factors:

* The evolution of **WebAssembly’s async and stack-switching support**
* How Wasm hosts (browsers, runtimes, engines) expose scheduling and suspension
* How Beanstalk’s build system configures and integrates the Wasm environment with the host platform

In the future, the Beanstalk compiler and build system are expected to:

* Lower coroutines into Wasm-compatible state machines
* Integrate with host event loops (e.g. browsers, game engines)
* Provide predictable, portable async behavior across environments

The syntax and semantics described here are intended to remain stable even as the underlying Wasm mechanisms evolve.

---

## In short

Beanstalk treats async as **structured control flow plus safe message passing**, not as magic. The primitives:

* `=>` create channels
* `>>` send values
* `<<` receive values
* `yield` suspends execution
* `#async` defines a concurrency boundary

Together, they form a small, explicit, and powerful foundation for async programming — one that scales from simple scripts to complex, concurrent systems without sacrificing clarity or safety.


]