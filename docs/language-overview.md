# Beanstalk Language Design Guide
Beanstalk is a programming language and build system designed for modern UI driven apps and webpages.

The design principles are:
- Powerful and flexible string templates for rendering content or describing UI
- Minimal and consistent syntax. Simple to learn and reason about
- Fast compile times for hot reloading dev builds that can give quick feedback for UI heavy projects
- Memory Safe with fallback GC that can eventually be statically optimised out with borrow checker rules
- Strict, statically typed and opinionated about doing things in as few ways as possible as concisely as possible

## Related references
This document describes Beanstalk syntax and user-facing semantics.

Use:
- `docs/compiler-design-overview.md` for compiler stage ownership and cross-stage data flow
- `docs/memory-management-design.md` for GC fallback, ownership optimisation, and borrow-analysis strategy
- `docs/src/docs/progress/#page.bst` for current implementation status
- `docs/roadmap/roadmap.md` for planned work

## Syntax Summary
For developers coming from most other languages, 
here are some key idiosyncrasies:

- Colon opens a scope, semicolon closes it. Semicolon does not end statements
- Square brackets are NOT used for arrays, curly braces are used instead
Square brackets are only used for string templates.
- Equality and other logical operators use keywords like "is" and "not" 
(you can't use == or ! for example)
- ~ tilde symbol to indicate mutability (mutability must be explicit).
This comes before the type if there is an explicit type declaration
- Double dashes for single line comments (--)
- Immutable reference semantics are the default for all stack and heap allocated types
- `#` is a binding-mode marker for compile-time constants, as in `name #= value`
- All copies have to be explicit unless they are used in part of a new expression (includes integers, floats and bools)
- Parameters and struct definitions use vertical pipes | 
- Result types are created with the '!' symbol. Options use '?'
- Function parameters and struct fields can define default values with `=`.
- `as` is used for three renaming domains:
  1. Type aliases: `AliasName as ExistingType`
  2. Namespace import aliases: `import @path as local_name`
  3. Grouped import per-entry aliases: `import @path { symbol as local_name }`
- Shadowing is not allowed. A name may not be redeclared while an existing binding with that name is visible.

### Names and shadowing
- Types/Objects/Choices/Type aliases: `PascalCase`
- Variables/functions: `regular_snake_case`

Beanstalk does not allow shadowing. This keeps each name mapped to one binding in its visible scope and avoids accidental rebinding.

Note: if immutable reassignment is currently rejected, adjust the first example to avoid implying normal `=` reassignment is accepted. Safer version:

```beanstalk
value ~= 1
value = 2 -- assignment to the existing mutable binding
```

## Core Syntax Patterns

```beanstalk
    int ~= 0
    float ~= 0.0

    -- You could also create a float with an explicit Type like this:
    float Float ~= 1 + 1

    -- Note: This is not a mutable heap allocated string, just a slice
    -- Regular strings are created using string templates
    -- Mutability here means that string_slice_value can be reassigned with another string slice
    string_slice_value ~= "text"
    raw_string_slice ~= `hi`
    
    char ~= '😊'
    
    -- Owned mutable string
    string_template ~= [:
        This is the Beanstalk programming language
    ]

    bool ~= true

    mutable_collection ~{Int} = {}
    immutable_collection {Int} = {}

    Struct = |
        value Float,
        another_value String,
    |
    
    instance ~= Struct(1.2, "hey")

    -- Notice the double colon
    Choice ::
        Option1,
        Option2 |
            value String,
        |,
        Option3 |
            inner_value String,
            another_value Float,
        |,
    ;
```

**Function definitions:**

```beanstalk
-- Basic function pattern
function_name |param Int| -> Int:
    -- 4-space indentation
    value = param + 1
    return value
;

describe |prefix String = "item", subject String| -> String:
    return prefix + ":" + subject
;
```

### Function Call Arguments
Named call arguments use `parameter = value`. Access mode is chosen at the call site.

```beanstalk
sum(values)                 -- positional shared
sum(~values)                -- positional mutable/exclusive
sum(items = values)         -- named shared
sum(items = ~values)        -- named mutable/exclusive
```

Rules:
- Function-call mutability is explicit at the call site.
- A parameter declared as `~T` accepts either:
  * `~place` for mutable/exclusive access to an existing place, or
  * a plain fresh rvalue (literal, template, constructor call, computed value).
- Passing an existing place to a mutable/exclusive parameter without `~` is an error.
- `~` is place-only syntax. Using `~` on an immutable binding, literal, template, constructor call, or computed expression is an error. Pass fresh values without `~`.
- Collections follow the same rule. Mutating collection operations do not get a permissive exception.
- Positional arguments must come before named arguments.
- No positional arguments are allowed after the first named argument.
- Each parameter can be provided only once.
- Host function calls and builtin member calls are currently positional-only.
- Defaulted parameters may be omitted. Named arguments let calls skip earlier
  defaulted parameters while still supplying later required parameters.

Variable mutability declarations and call-site mutable access are separate concepts:
- `value ~= ...` declares or reassigns a mutable binding.
- `fn(~value)` or `fn(param = ~value)` requests mutable/exclusive access for one specific call argument.
- A mutable binding does not automatically satisfy a mutable parameter. Existing places still require `~` at the call site.

```beanstalk
describe |prefix String = "item", subject String| -> String:
    return prefix + ":" + subject
;

label = describe(subject = "apple") -- "item:apple"
```

Struct fields use the same default-value syntax. Struct constructors can omit
defaulted fields, and named constructor arguments can skip defaults in any
declaration position.

```beanstalk
Config = |
    height Int = 100,
    width Int,
|

default_height = Config(width = 80)
full = Config(width = 80, height = 120)
```

Choice payload fields do not support default values in the current Alpha surface.

### Numeric Semantics
- Whole-number literals are `Int`.
- Decimal-point literals are `Float`.
- `+`, `-`, `*`, and `%` preserve `Int` when both operands are `Int`.
- `/` is always real division. `Int / Int` evaluates to `Float`.
- `//` is integer division. It currently requires `Int // Int` and evaluates to `Int`.
- Integer division uses truncation toward zero (`-5 // 2` -> `-2`).
- Mixed `Int`/`Float` arithmetic for regular operators evaluates to `Float`.
- There is no implicit `Float -> Int` coercion.
- Use `//` for integer division.
- Use `Int(...)` when an explicit conversion is required.

### Results and Options
Beanstalk supports optional values and error returns with compact syntax.
`none` is parse-context-sensitive: it requires an optional surrounding type context rather than being recovered later by post-parse coercion.

Optional types use `?`:

```beanstalk
name String? = none

find_name |id String| -> String?:
    if id.is_empty():
        return none
    ;

    return "Alice"
;
```

A normal value of type `T` can be used where `T?` is expected.  
`none` is the only special option value.

Error-returning functions mark one return slot with `!`:

`Error` is a builtin language type with this public shape:

- `message String`
- `code Int = 0`

`Error` is reserved and cannot be re-declared by user code. `ErrorKind`,
`ErrorLocation`, and `StackFrame` are ordinary user-available names.

```beanstalk
parse_number |text String| -> Int, Error!:
    if text.is_empty():
        return! Error("Missing number", 200)
    ;

    return 42
;
```

Error-returning calls must be handled at the call site.

Bubble the error up:

```beanstalk
value = parse_number(text)!
```

Or provide fallback values:

```beanstalk
value = parse_number(text) catch:
    then 0
;
```

`then` is the local value-production terminator for block forms that are used at
an explicit value-receiving site. It returns one or more expressions from the
nearest active value-producing block to the containing declaration, assignment,
return, multi-bind, or outer `then` site.

Current value-producing `if`, full match, and block-form `catch:` /
`catch |err|:` recovery reuse the same mechanism. Nested ordinary branch paths
inside a value-producing catch can target the surrounding catch block with
`then`.

Value-producing blocks are intentionally not general expressions. They are valid
only at closed receiving sites such as declarations, assignments, multi-bind,
returns, and nested `then` values. They are rejected in function arguments,
operator operands, constructor arguments, collection literals, template
interpolation, and ordinary expression statements.

Inline value forms are single-logical-line sugar:

```beanstalk
value = if condition then 1 else 0
name = if maybe_name is |name| then name else "guest"
fallback = parse_number(text) catch then 0
```

Block value forms use `then` in every value-producing path:

```beanstalk
name = if maybe_name is |name|:
    then name
else
    then "guest"
;
```

Multi-value blocks must produce the same arity on every producing path, and that
arity must match the receiver:

```beanstalk
name, score = load_user(id) catch |err|:
    io(err.message)
    then "guest", 0.0
;
```

Results remain call-site-only. `catch:` and `catch |err|:` are the supported
result recovery forms, and public first-class `Result` values or result pattern
matching remain deferred. Use `!` return slots and call-site handling instead.

Assertions are statement-only invariant checks:

```beanstalk
assert(index < items.length)
assert(index < items.length, "index must be in bounds")
assert(false, "unimplemented backend path")
```

`assert` is a language-owned statement intrinsic, not a function. It cannot be
assigned, passed as an argument, imported, aliased, or used in expression
position. Assertions are always checked. Failed assertions are unrecoverable,
do not return `Error!`, and cannot be caught with `catch`. Expected failures
should use typed error propagation with `Error!` and `catch`; assertion failure
is the current explicit panic path for programmer invariants.

`assert(false)` and `assert(false, "message")` are statically terminal and may
end a non-`Void` function. A dynamic `assert(condition)` is not statically
terminal because the pass path continues normally. Assertion messages currently
must be string literals; runtime and compile-time constant message expressions
remain deferred.

Options are regular values. The canonical value-recovery form is explicit
present-value inspection:

```beanstalk
display_name = if maybe_name is |name| then name else "guest"
```

Statement-only absence inspection remains valid:

```beanstalk
if maybe_name is none:
    io("missing")
;
```

Full option matches support `none`, literal or relational present-value
patterns, `|value|` present capture, guards, and `else` exhaustiveness:

```beanstalk
label = if maybe_name is:
    "Ada" => then "Hi Mum"
    |name| => then name
    none => then "guest"
;
```

Direct option fallback syntax such as `maybe_name else then "guest"` is rejected
in favour of the canonical `if option is |value| ... else ...` form.

Postfix `?` on an optional expression is propagation, not recovery. It unwraps a
present value and returns `none` from the current function when the expression is
absent:

```beanstalk
get_display_name |id String| -> String?:
    user = find_user(id)?
    return user.name
;
```

Multiple success values use the normal return list and a shared assignment on the caller side:

```beanstalk
pair || -> String, Int:
    return "Ana", 2
;

name, count = pair()
```

Multi-bind accepts explicit multi-return function-call results and value-producing
blocks at closed RHS receiving sites. Regular declarations remain single-target,
and user-visible tuple values are not supported.

Named handler scopes are supported for explicit error-handling blocks, including fallback values when the success path still needs values:

```beanstalk
name, score = load_user(id) catch |err|:
    io(err.message)
    then "guest", 0.0
;
```

Beanstalk still uses multiple returns, so the success path keeps normal return values. The special `!` return is only for the error path.

### Collections
Collections are ordered groups of values that are zero-indexed.

Collection literals are homogeneous. A non-empty collection literal infers its element type from
its items. Empty collection literals require an explicit collection type annotation because their
element type is not immediately inferable.

```beanstalk
values ~= {'a', 'b', 'c'}  -- inferred as {Char}
empty_values ~{Int} = {}   -- explicit empty Int collection

values ~= {}               -- Type error: element type is ambiguous
mixed ~= {1, "bad"}        -- Type error: inconsistent item types
```

Beanstalk does not infer an empty collection's element type from later `push`, assignment, loop,
function argument, HIR, or borrow-analysis use. A declaration's type must be explicit at the
declaration site or immediately inferable from its initializer.

A collection binding declared with the mutable symbol can be mutated through collection methods.

`set`, `push`, and `remove` are mutating collection operations and require explicit mutable/exclusive receiver access at the call site.
Collections do not get a permissive exception: mutating collection operations follow the same explicit call-site mutability rules as user-defined mutable parameters.

```beanstalk
items ~= {10, 20, 30}
~items.push(40)
~items.set(0, 99) catch:
;
removed = ~items.remove(1) catch:
    then 0
;
```

`collection.get(index)` returns `Elem, Error!`, so value-position reads must be
handled.

`get`, `set`, and `remove` are fallible collection operations. `push` and
`length` are infallible. Invalid receivers or out-of-bounds indices for fallible
operations produce structured errors rather than silent no-ops.

Use `set(index, value)` for indexed writes:

```beanstalk
~items.set(0, value) catch:
;
```

Indexed assignment through `get` has been removed. Use
`~items.set(index, value)` instead.

There may not be a runtime call under the hood when using collection methods, because the compiler can lower these operations directly.

### Standard Output
```beanstalk
-- Print to stdout
io("Hello, World!")

-- Print with variables
message = "Hello"
io(message)

-- Print with interpolation using templates
name = "Alice"
io([: Hello, [name]])

-- Print in functions
greet |name String|:
    io([:Hi [name], how's it oing?])
;
```

## String Template System
**Templates use `[]` exclusively** - never confuse with collections `{}`.

Templates are either folded to strings at compile time, or become functions that return strings at runtime. They are the ONLY way to create mutable strings in Beanstalk. `""` are only for string slices.

Template structure:
- Head and body separated by `:`
- Variable capture from the surrounding scope

Templates unlock the full power of Beanstalk's HTML / CSS generation capabilities.

### Template Styles
Templates can be used to build complex UI components. They can use slots to insert content from other templates and have **style metadata** attached to them.

In the HTML project builder, only direct top-level template expressions in the entry file contribute page fragments.
Top-level runtime templates are evaluated by the entry `start()` function in source order, while top-level const templates are folded at compile time and merged separately.
Templates assigned to variables or returned from functions do not contribute page fragments by themselves.

A template’s style is defined in the **template head** using `$` directives. 
`$` introduces **compiler-handled directives** (so they don’t collide with normal variables and can be extended by the build system), such as formatter-like built-ins, precedence controls and default child templates that are automatically applied to direct child templates.

Directive availability is frontend-registry based:
- Frontend built-ins are available by default (`$markdown`, `$code`, `$raw`, `$slot`, etc.)
- Project builders can register additional project-specific directives using the same `$name` syntax. In the HTML project, that includes `$html`, `$css`, and `$escape_html`
- Unknown directives fail as syntax/rule errors unless they are registered

```beanstalk
-- Define a template style
[
  $markdown,                        
  $children([: All children start with this prefix ])    -- Applies only to direct children
:
  # Hello
  This template is parsed as markdown.

  @example.com (Here is a link!) using this custom markdown flavour.

  [$todo: write some more info]

  [: This child is prefixed]
]
```

**Frontend Built-in Style Directives**

- $slot / $insert(..) - See slots below
- $fresh              - Opts this child template out of wrappers applied by the immediate parent's `$children(..)` directive
- $markdown           - Parses the template bodies with a custom flavour of Markdown
- $raw                - Preserves authored template body whitespace exactly
- $note / $todo       - Comments (ignored by final output)
- $doc                - Turns the template into a documentation comment
- $children(..)       - Accepts a template (or string slice) that will be applied only to this template's direct child templates

For `$children(..)` template arguments, the child template must close with `]` before the directive closes with `)`.

**HTML Project Directives**

- $html               - Parses the template body as raw HTML (no escaping)
- $css                - Provides some basic warnings for malformed CSS
- $escape_html        - Escapes HTML-sensitive characters in the template body
- $code               - Highlights code blocks, wrapping keywords and symbols in spans

Formatting directives do not automatically flow into nested child templates.
If a child template should keep using a formatter such as `$markdown`, redeclare it in that child template's head.

`$fresh` is per-child and only affects wrapper application from the immediate parent. Siblings without `$fresh` still receive the parent wrappers:

```beanstalk
list #= [$children([:<li>[$slot]</li>]):
  <ul>
    [$slot]
  </ul>
]

[list:
  [: one ]
  [$fresh: [: two ]]
]
```

In this example, `one` is wrapped with `<li>...</li>`, while `two` opts out and is rendered without the parent `$children(..)` wrapper.

### Template Slots
Template slots let one template receive content from another template. The default slot is written as `[$slot]` and marks where the main body content should be inserted.

Named slots can also be declared with `[$slot("name")]`. These allow helper templates to insert content into a specific part of another template using `$insert("name")`.

Positional slots can be declared using positive integers, such as `[$slot(1)]`, `[$slot(2)]`, etc. Loose contributions (those not explicitly targetting a named slot) are assigned to positional slots first, in ascending numeric order. Any remaining loose contributions go to the default slot if it exists.

```beanstalk
img = [:
    <img src="[$slot(1)]" alt="[$slot]">
]

[img, "logo.png": Site logo]
```

In this example, `"logo.png"` fills the first positional slot `[$slot(1)]`, and `"Site logo"` fills the default slot `[$slot]`.

Named inserts:
```beanstalk
title = [:
    <h1 style="[$slot("style")]">
        [$slot]
    </h1>
]

blue = [$insert("style"): color: blue;]

[title, blue:
    Hello world
]
```

In this example, `blue` inserts `color: blue;` into the `style` slot of `title`, while `Hello world` is inserted into the default slot.

If a template has named or positional slots but no default slot, any loose body content that cannot be assigned to a positional slot is an error. 
If a slot receives no content, it expands to an empty string.
Repeated slots, such as two occurrences of `[$slot(1)]`, will replay the same content in both places.

Slot applications may contain runtime-producing content. Runtime-capable
templates support default, named, positional, and loose contributions from
template `if` branches and template `loop` bodies. Valid runtime slot
applications are routed during AST preparation, then lowered through ordinary
runtime string accumulators. Missing slots still render as empty strings, and
repeated slots replay the same accumulated contribution.

```beanstalk
card = [:
    <h1>[$slot("title")]</h1>
    <section>[$slot]</section>
]

[card:
    [$insert("title"):
        [if maybe_title is |title|:
            [title]
        [else]
            Untitled
        ]
    ]

    [loop items |item|:
        [item]
    ]
]
```

Because `$children(..)` only applies to direct children, nested helpers can scope row and cell wrappers independently:

```beanstalk
table #= [$children([:<tr>[$slot]</tr>]):
  <table style="[$slot("style")]">
    [$children([:<td>[$slot]</td>]):[$slot]]
  </table>
]

[table:
    [: [: Type] [: Description] ]
    [: [: float ] [: 64 bit floating point number] ]
]
```

In that example, each direct child of `table` becomes a row, while each direct child of a row becomes a cell. The outer `<tr>` wrapper does not leak into the inner cell templates.

### Template Head Suffix Control Flow
Templates support `if` and `loop` as final head suffixes immediately before the
body colon. If the head already contains a value, helper, or directive, the
control-flow suffix requires a comma.

```beanstalk
[if show:
    Visible
]

[card, if show:
    Visible inside card
]
```

Template `if` supports `Bool` conditions, option-present capture, and
standalone `[else if ...]` sentinels. `[else]` is optional, must be standalone,
and belongs to the nearest active template `if`. Full pattern-match branch
chains belong to ordinary statement/value `if value is:` blocks, not template
heads.

```beanstalk
maybe_name String? = none
use_fallback = true

[if maybe_name is |name|:
    Hello [name]
[else if use_fallback]
    Hello fallback
[else]
    Hello guest
]
```

Template `loop` supports conditional loops, collection iteration, and numeric
range iteration using the normal loop-header syntax. Conditional template loops
do not take bindings. Collection and range loops can use a second binding for
the zero-based index. Iterations concatenate directly with no implicit
separator. Standalone `[break]` and `[continue]` sentinels are valid inside
template loop bodies, including nested template `if` and `else if` bodies. They
target the nearest active template loop and are structural control
signals, not renderable output.

```beanstalk
[loop has_next():
    [next_item()]
]

[loop items |item, index|:
    [index]: [item]
]

[loop 0 to 10 |i|:
    [i]
]

[loop items |item|:
    [if item.skip:
        [continue]
    ]
    [if item.done:
        [break]
    ]
    [item]
]
```

Head values and wrappers apply to selected/generated output. For
`[head, loop ...:]`, the head wraps the whole aggregate once. Put
per-iteration wrappers inside the loop body. False/no-else branches and
false conditional loops or zero-iteration range/collection loops produce
structural no-output and skip shared wrappers. Loop iterations that `continue`
or `break` before any output also do not count as structurally emitted, while
output before a loop-control sentinel is preserved.

Top-level `#[if ...:]` and `#[loop ...:]` fragments must fully fold at compile
time. Const-required template `if` validates every branch body.
Const-required conditional template loops fold to no-output when their condition
is compile-time `false`; compile-time `true` and unknown/runtime conditions are
rejected because no template loop termination analysis is performed. Const range
and collection template loops can fold structural `[break]` and `[continue]`,
and use the `template_const_loop_iteration_limit` project config guard. The
default is `10_000` iterations per const template loop, and the configured value
must be a positive folded `Int` no greater than `1_000_000`. Runtime template
control flow is lazy: only the selected `if` branch evaluates, range and
collection loop sources are evaluated once before iteration, and conditional
loop conditions are evaluated before every iteration.

Runtime slot applications are valid inside template control flow after normal
slot routing. Escaped helper artifacts that still leave unresolved `[$slot]` or
`$insert(...)` output inside runtime control-flow bodies are invalid template
structure.

### If Statements / Pattern Matching
If statements are non-exhaustive and don't have 'else if'.
The else branch is optional.

```beanstalk
if value is true:
    io("then branch")
else
    io("else branch")
;
```

Pattern matching is exhaustive and uses `if <value> is:` with one or more pattern arms.

```beanstalk
value ~= 2
allow ~= false
result ~= "unset"

if value is:
    1 => result = "one"
    2 if allow => result = "guarded-two"
    2 => result = "two"
    else => result = "fallback"
;
```

- Arms are delimited by the next line-initial arm, `else =>`, or the final match-closing `;`
- Per-arm semicolons are invalid in match blocks
- A non-default arm starts with `<pattern> => <body>` or `<pattern> if <bool_expr> => <body>`
- Match arm headers must start at the beginning of a logical line
- Guard expressions (`if <bool_expr>`) must be `Bool`
- For non-choice scrutinees, `else =>` is required
- For choice scrutinees:
  - `else =>` always satisfies exhaustiveness
  - Without `else =>`, every variant must be covered
  - If any arm has a guard, `else =>` is required
  - The same variant cannot be matched more than once

Arm syntax:
- `<pattern> => <body>`
- `<pattern> if <bool_expr> => <body>`
- `else => <body>`

Currently supported patterns:

- Literal patterns: `1 =>`, `"ok" =>`, `true =>`
- Choice variant patterns: `Ready =>` or `Status::Ready =>`
- Choice payload capture patterns: `Err(message) =>` or `Pending(retry_count, message) =>`
- General capture patterns: `captured =>` binds the whole scrutinee value to `captured`
- Relational patterns for ordered scalar values: `< 0 =>`, `<= 10 =>`, `> 0 =>`, `>= 100 =>`

The catch-all default is expressed only through `else =>`.

Capture names in payload patterns must exactly match the declared field names.
Choice payload captures may be renamed with `as`: `Err(message as error_text) =>` binds the payload field to a different local name visible only in the guard and body of that arm.
Exhaustiveness is tag-level: a payload capture arm covers all values of that variant regardless of payload content.

Relational patterns are supported for ordered scalar scrutinees such as `Int`, `Float`, `Char`, and `String`.
The pattern value must be a literal of the same compatible type.
String ordering is backend-defined for Alpha (JavaScript string comparison for the JS backend).

```beanstalk
value ~= 12

if value is:
    < 0 => io("negative")
    0 => io("zero")
    <= 10 => io("small")
    else => io("large")
;
```

Relational string pattern example:

```beanstalk
name ~= "alice"

if name is:
    < "m" => io("before m in alphabet")
    else => io("m or after")
;
```

Capture pattern example:

```beanstalk
value ~= 42

if value is:
    captured => io(captured.to_string())
    else => io("fallback")
;
```

Nested choice payload patterns (for example matching inside a payload field) are deferred. Use `else =>` or simple payload captures for choice values instead.

Choice default example:

```beanstalk
Status ::
    Ready,
    Loading,
    Failed,
;

status ~= Status::Loading

if status is:
    Ready => io("ready")
    else => io("not ready")
;
```

## Loops
Beanstalk uses a **single** loop keyword: `loop`.

* **`to`** / **`to &`** select range semantics (exclusive vs inclusive)
* **Range loops can bind the current counter**
* **Collection loops can bind the current item**
* **An optional second binding provides the zero-based index**
* **No `reverse` keyword**; direction is inferred from the bounds
* **`by`** controls step size and works with both directions
* **Loop bindings use `|...|` and come after the loop condition**
* **The binding brackets are optional** when the current item / counter is not needed

Loops come in two forms:

1. **Conditional loops** (repeat while a condition is true)
2. **Iteration loops** (iterate over a collection or a numeric range)

### Conditional loops
A conditional loop repeats for as long as its condition stays true.

```beanstalk
loop is_connected():
    io("still connected")
;
```

Conditional loops usually do not need bindings, so the parameter brackets are normally omitted.

### Iteration loops
Iteration loops evaluate the loop condition as an iterable source and optionally bind values for each iteration.

Iteration can step through either:

* a **collection** (yielding the current item), or
* a **range** (yielding the current counter)

The loop bindings are written after the loop condition using `|...|`.

```beanstalk
loop items |item|:
    io(item.to_string())
;
```

A second binding can be added to receive the current zero-based index:

```beanstalk
loop items |item, index|:
    io([index]: [item])
;
```

If the current item or counter is not needed, omit bindings entirely:

```beanstalk
loop items:
    io("next item")
;
```

### Range loops
If the loop header contains **`to`**, it is a range loop.

* `to` uses an **exclusive** end bound
* `to &` uses an **inclusive** end bound

You can omit the leading `0` before `to` as sugar:

```beanstalk
loop to 10 |i|:
    io([i])
;
-- equivalent to: loop 0 to 10 |i|:

loop to & 10 |i|:
    io([i])
;
-- equivalent to: loop 0 to & 10 |i|:
```

```beanstalk
loop 0 to 5 |i|:
    io([i])
;
-- yields: 0, 1, 2, 3, 4

loop 0 to & 3 |i|:
    io([i])
;
-- yields: 0, 1, 2, 3
```

You can specify a step using `by`.

```beanstalk
loop 0 to 8 by 2 |i|:
    io([i])
;
-- yields: 0, 2, 4, 6
```

The binding is optional here too:

```beanstalk
loop 0 to 3:
    io("tick")
;
```

### Direction is inferred from bounds
Beanstalk automatically determines the iteration direction from the bounds:

* If `start < end`, the default direction is ascending
* If `start > end`, the default direction is descending

With no `by`, the default step is `+1` for ascending ranges and `-1` for descending ranges.

```beanstalk
loop 4 to 0 |i|:
    io([i])
;
-- yields: 4, 3, 2, 1
```

You can also supply an explicit step:

```beanstalk
loop 6 to & 0 by 2 |i|:
    io([i])
;
-- yields: 6, 4, 2, 0
```

- When the bounds imply descending iteration, `by` is treated as a magnitude and the compiler applies the correct sign automatically.
- A step of `0` is invalid.

Float ranges are supported, but **`by` should be considered required** to avoid ambiguous or non-terminating loops.

```beanstalk
loop 0.0 to 1.0 by 0.1 |t|:
    io([i])
;
```

## Structs

```beanstalk
    -- To create a new instance of this struct, it must have 2 parameters passed in,
    -- a string and an integer
    Person = |
        name String,
        age Int,
    |

    -- Create a new instance of the struct
    person ~= Person("Alice", 30)

    -- Access fields using dot notation
    io(person.name) -- "Alice"
    io(person.age)  -- 30

    -- Defining a struct, then defining a receiver method for it
    Vector2 = |
        x Float,
        y Float,
    |

    reset |this ~Vector2|:
        this.x = 0
        this.y = 0
    ;

    vec ~= Vector2(x = 12, y = 87)
    ~vec.reset()
```

Runtime structs are nominal types. Matching field shapes do not make two structs interchangeable. Type aliases to structs do not create a new struct identity; the alias is transparently the same type as the target struct.

Receiver methods in v1 are statically resolved:
- `this` is a reserved word. It can only appear as the first parameter of a receiver method and inside that method's body. It cannot be used as a normal variable, field, function name, loop binding, or top-level declaration.
- A method is a top-level function whose first parameter is named `this`
- There may be exactly one `this` parameter
- Supported receiver types are user-defined structs and built-in scalars (`Int`, `Float`, `Bool`, `String`)
- Collection built-ins (`get`, `set`, `push`, `remove`, `length`) are compiler-owned operations and are not declared via `this`
- `this T` declares an immutable receiver
- `this ~T` declares a mutable receiver
- Mutable receiver calls must spell mutable/exclusive access at the receiver site; a mutable binding alone is not enough.
- Methods are called with receiver syntax only: `value.method(...)`
- `method(value, ...)` is not valid for receiver methods
- Mutable receiver methods require a mutable place receiver, so temporaries and rvalues cannot be mutated through method syntax
- Field writes follow the same mutable-place rule as mutable methods

User-defined struct methods must be declared in the same file as the struct definition. This same-file restriction does not apply to built-in scalar receivers.

Exported receiver methods become available through the receiver type, not as free-function imports.
Namespace imports make receiver methods from that import surface available through receiver-call syntax, but not as namespace fields. Grouped imports may import and alias receiver methods, and the alias participates in the same file-local collision policy as other visible names. Receiver-method names can be shared by receiver methods because dispatch includes the receiver type, but they still collide with ordinary value, type, namespace, prelude, and builtin names. Importing a receiver type automatically imports receiver methods for that type from the same import surface. For JS-backed external packages, this uses the exact package-scoped opaque receiver type declared by `@bst.opaque`. A grouped receiver method import is invalid unless its receiver type is visible in the importing file, either through a type import in the same grouped import, an earlier visible type import, a namespace import from that surface, or an exact transparent type alias.

```beanstalk
import @web/canvas { Canvas2d, fill_rect }
-- also valid: import @web/canvas { fill_rect, Canvas2d }

import @web/canvas { fill_rect } -- invalid: Canvas2d is not visible
```
Type aliases in a `#mod.bst` facade do not automatically re-export private implementation methods; public method behavior must be exposed by authored facade declarations.

```beanstalk
double |this Int| -> Int:
    return this + this
;

value = 21
io(value.double()) -- 42
```

## Choices
Choices are nominal tagged unions. Each variant is either a unit variant or a record payload variant.

### Unit choice declaration

```beanstalk
Status :: Ready, Busy;
```

### Payload choice declaration

```beanstalk
Result ::
    Ok,
    Err | message String, code Int |,
;
```

### Constructors
Unit variants are constructed with `Choice::Variant`. Payload variants are constructed with `Choice::Variant(...)` using positional or named arguments.

```beanstalk
success = Result::Ok

-- Positional constructor
failure = Result::Err("bad request", 400)

-- Named constructor
named = Result::Err(message = "bad request", code = 400)
```

### Structural equality
Two choice values are structurally equal when they share the same choice type, the same variant, and every payload field is equal in declaration order. Choice equality is only supported when **every** payload field type across **all** variants supports structural equality.

Supported payload field types for structural equality:
- `Int`, `Float`, `Bool`, `Char`, `String`
- Other choices whose payload fields all support equality
- built-in options when their inner types support equality

Unsupported field types reject the comparison with a diagnostic:
- Structs, collections, functions, fallible result carriers, external opaque types, and templates do not support structural equality.

Unit variants compare by variant identity:

```beanstalk
Status :: Ready, Busy;

if Status::Ready is Status::Busy:
    io("never true")
;
```

Constructed payload choices can be compared directly:

```beanstalk
Result :: Ok, Err | message String |;

if Result::Err("bad") is Result::Err("bad"):
    io("equal")
;
```

### Payload immutability
Payload fields are immutable after construction. Direct payload field access is deferred and produces a diagnostic suggesting pattern matching. Payload field mutation is rejected with an immutability diagnostic.

## Type aliases
Type aliases give another name to an existing type at compile-time.
They can target built-in types, structs, choices, options, collections, imported types, and external package types.

```beanstalk
UserId as Int
Names as {String}
MaybeName as String?
```

Type aliases are **transparent**. They do not create a new nominal type.

```beanstalk
UserId as Int

id UserId = 42
raw Int = id -- valid, UserId is Int
```

Imported types can be renamed with import aliases, and type aliases can target those imported aliases:

```beanstalk
import @types { UserId as Id }

LocalId as Id
value LocalId = 1
```

## Module System and Imports
A module is a directory-scoped unit of Beanstalk source files compiled together into a single output. A directory is treated as a module root when it contains one or more `#*.bst` files (excluding `#config.bst`).

A project is one or more of these modules together with libraries and sometimes other file types compiled into a larger output.

At the root of every project is a `#config.bst` file.
`#config.bst` uses normal Beanstalk declaration syntax. Stage 0 compiles it through AST, then extracts folded immutable declarations for known config keys from top-level declarations and the implicit config start body.

Compiler-stage details for module discovery and dependency sorting are described in `docs/compiler-design-overview.md`.

Example:
```beanstalk
project = "html"
entry_root = "src"
dev_folder = "dev"
output_folder = "release"
library_folders = {"lib", "packages"}
```

Config accepts only known config-key value declarations authored in `#config.bst`.
Each value must resolve to a compile-time constant through the same AST const facts used by the rest of the frontend.
Plain immutable key declarations and explicit `#` key constants are both accepted.
Config values may reference earlier compile-time config keys or constants imported from core/builder source libraries.

`#config.bst` may also contain core/builder imports, type aliases, structs, and choices as compile-time support declarations.
Every authored value-producing declaration must still be a known config key; config-local helper constants remain deferred.
Authored config imports may only target core or builder-provided libraries.
Project-local imports and relative imports from the authored config file are rejected by design.

Known keys have strict value shapes before they are applied or stored:
- string settings accept string literals and folded templates;
- `project` is currently a closed string set accepting only `"html"`;
- boolean HTML settings such as `redirect_index_html` require folded `Bool` values, not strings such as `"false"`;
- `library_folders` accepts either one string folder name or a collection of string folder names;
- `template_const_loop_iteration_limit` requires a positive folded `Int`, defaults to `10_000`,
  and is capped at `1_000_000`.

Explicit `#` config-key constants can use const-record field projection when the expression fully folds, for example `entry_root #= Defaults().entry_root`.
The same projection in a plain `=` config key is deferred.
Structured typed config values such as `project = Project::Html(...)` remain deferred.

Mutable bindings, functions, calls, host calls, runtime statements, non-key helper constants, standalone templates, and `#[...]` page fragments are rejected in authored config.

**Import syntax:**
```beanstalk
-- Import a file or package namespace:
import @path/to/file
import @core/math as math
import @vendor/drawing.js as drawing

-- Import one exported symbol with its original name:
import @path/to/file {symbol}

-- Import with a file-local alias:
import @path/to/file {symbol as local_name}

-- Grouped imports can alias individual entries:
import @components {
    render as render_component,
    Button as UiButton,
    Card,
}

-- Nested grouped entries can alias the final imported symbol:
import @docs {
    pages/home {render as render_home},
    pages/about {render as render_about},
}
```

Import rules:
- Imports target exported symbols, not file-level start functions
- Namespace imports such as `import @path/to/file` create shallow, field-access-only import records
- An alias applies only in the importing file. It does not change the canonical declaration path
- Import aliases are not re-exported. A facade that wants to expose imported behavior must declare a real wrapper declaration
- Alias names cannot collide with any visible name in the same file: same-file declarations, other imports, prelude symbols, builtins, or type aliases
- Aliases should preserve the leading-case convention of the imported symbol. A mismatch warns (for example, `User as user` or `render as Render`)
- Grouped imports cannot use a trailing group-level alias. Alias individual entries instead:
  `import @components { render as render_component }`
- Direct symbol-path imports such as `import @core/math/sin` are invalid. Use grouped syntax or a namespace import.
- Import records are not first-class values; use `namespace.member` in value position or `namespace.TypeName` in type position.
- Import records are shallow. Import child paths directly instead of traversing nested path segments through fields.
- `.bst` source imports are extensionless. `import @ui/button.bst` is invalid.
- Direct project/local JavaScript imports require their `.js` extension and a builder with a `.js` external import provider.
- If a namespace import's final path stem is not a valid Beanstalk identifier, use an explicit alias such as `import @vendor/my-canvas.js as my_canvas`.
- Direct imports of special files such as `#mod.bst`, `#page.bst`, and `#config.bst` are invalid.

### Module roots, entry files, and facades
- A module root may contain multiple `#*.bst` files with different build-system roles (for example `#page.bst` and `#mod.bst`)
- Build-system entry files such as `#page.bst` own top-level runtime/start code
- `#mod.bst` is the only outward-facing export surface for a module
- A module root without `#mod.bst` exports nothing outside itself

**Entry files and implicit start functions:**
- The module entry file has an implicit `start` function containing its top-level runtime code
- Only the entry file executes top-level runtime code automatically
- Non-entry files may contain imports and top-level declarations, but not top-level executable statements
- The implicit `start` function is build-system-only and cannot be imported or called directly from Beanstalk code

**File execution semantics:**
```beanstalk
-- main.bst (entry file)
import @utils/helper {run_helper}
import @utils/helper {another_func}

io("Starting main")

run_helper()
another_func()
```

Only the entry file's top-level runtime code executes automatically.
Other files contribute declarations that must be imported explicitly by symbol.

**Import resolution rules:**
- Relative child imports such as `@./x` resolve from the importing file's directory
- Parent-directory imports with `..` are not supported
- Imports cannot escape module/library/project boundaries
- Non-relative imports whose first segment matches a source library prefix resolve from the corresponding library root
- Other non-relative imports resolve from the configured module entry root
- Config-defined library folders are scan roots. Each direct child directory becomes an import prefix. `/lib` is the default scan folder when `library_folders` is omitted
- Importing a folder across or into a module boundary requires that folder to expose a `#mod.bst` facade. Import concrete same-module files directly when no facade is intended.
- A sibling `name.bst` file and `name/` folder in the same source directory are rejected as an ambiguous import name. `.js` files are excluded from that specific `.bst`/folder collision rule.
- Grouped imports expand into multiple individual symbol imports
- Circular imports are detected and cause compilation errors

### Libraries and `#mod.bst`
Libraries and regular modules share the same visibility model. A source library is a normal module discovered through a library root.

Beanstalk has several library categories:
- Core prelude libraries: every builder must provide `@core/prelude`. Its exported prelude surface is available as bare names
- Core libraries: optional builder-provided packages such as `@core/math`, `@core/text`, `@core/random`, and `@core/time`
- Builder libraries: builder-owned libraries such as the HTML builder's `@html`
- Project libraries: project-local source libraries discovered through config-defined library folders (default convention: `/lib`)
- External packages: virtual packages implemented by backend metadata rather than `.bst` source files

Core libraries require explicit imports unless they are part of the prelude. A builder that does not expose a core package rejects imports from that package with an unsupported-by-builder diagnostic.

Source libraries are normal Beanstalk source files behind a library root. A source library module that exports outside itself must expose its public surface through `#mod.bst`.

```text
lib/
  ui/
    #mod.bst
    button.bst
```

```beanstalk
import @ui {button}
```

`#mod.bst` is an API facade, not a runtime entry or shared implementation file.
- `#mod.bst` may contain:
  - normal imports used by wrapper declarations
  - authored top-level functions, structs, choices, type aliases, and `#` constants
- `#mod.bst` may not contain:
  - top-level runtime statements
  - runtime templates/start-function code
  - `#[...]` page fragments

Access and visibility rules:
- Files inside the same module may import and use private implementation files according to normal internal module rules.
- Outside modules must import through the module facade surface exposed by `#mod.bst`
- Modules may contain submodules, but outside modules cannot bypass intermediate facades. Visibility flows through explicit facade exports

Facade files expose imported implementation behavior through real declarations:

```beanstalk
import @./button { Button, render_button }
import @core/math { PI }

AppButton as Button

primary_pi #= PI

render |button AppButton| -> String:
    return render_button(button)
;
```

Direct facade re-export syntax is not part of the Alpha surface. Legacy `#import` is rejected. Function alias exports and automatic method re-export through facade type aliases remain deferred.

Only `#mod.bst` creates a public module surface.

`#page.bst` may import from files in the same directory/module, but it does not export those declarations unless `#mod.bst` does.

`#config.bst` may affect build behavior, but it does not create language-visible imports.

### External platform package imports
Project builders may provide virtual packages such as `@core/io` or `@web/canvas`.
These are not Beanstalk source files. They expose typed external functions and opaque external types.

The implementation matrix is the source of truth for which external packages and backend targets are currently supported.

```beanstalk
import @core/math
import @core/math { sin as sine }

io("hello")
value = math.sin(1.0)
aliased = sine(1.0)
```

Some symbols may be imported automatically by the builder prelude. For normal builds, `io()`, `IO`, and the compiler-owned `Error` type are available without explicit imports.

Initial optional core packages:
- `@core/math`: constants `PI`, `TAU`, `E`, and Float math helpers.
- `@core/text`: `length`, `is_empty`, `contains`, `starts_with`, `ends_with`.
- `@core/random`: `random_float`, `random_int`. `random_int(min, max)` is inclusive at both ends and swaps bounds when `min > max`; seeded random is deferred.
- `@core/time`: `now_millis`, `now_seconds`. Date objects, timezones, formatting, durations, and monotonic clocks are deferred.

External types are opaque. They can be passed, returned, and used by external functions, but cannot be constructed with struct syntax or field-accessed by Beanstalk code.

The HTML builder also supports annotated single-file `.js` imports. JavaScript libraries expose Beanstalk symbols through `@bst.opaque` and `@bst.sig` comments, while the JavaScript export name remains the runtime implementation detail. Supported JavaScript export forms are `export function name(...) { ... }` and block-bodied arrow exports such as `export const name = (...) => { ... }`; expression-bodied arrow exports are rejected. In `@bst.sig`, `this` follows the Beanstalk receiver-method rule: it may appear only once and only as the first parameter.

```js
/**
 * @bst.opaque Canvas
 */

import { bstOk, bstErr } from "@beanstalk/runtime";

/**
 * @bst.sig get_canvas |id String| -> Canvas, Error!
 */
export function getCanvas(id) {
    const canvas = document.getElementById(id);
    return canvas ? bstOk(canvas) : bstErr(404, "Canvas not found");
}
```

Beanstalk code imports that file as a typed external package:

```beanstalk
import @./drawing.js as drawing

canvas_ref = drawing.get_canvas("game")!
```

`@bst.package` and unknown `@bst.*` annotations are not supported. Package identity comes from the project-local import path or from builder-owned Rust registration for virtual packages such as `@web/canvas`.

Only builder-registered runtime modules such as `@beanstalk/runtime` may be imported by a Beanstalk JS library. Runtime module imports must use named static imports whose imported names are exported by the registered module, for example `import { bstOk, bstErr } from "@beanstalk/runtime";`. The named import list may span multiple lines. Default, namespace, side-effect, aliased runtime imports, and unknown runtime export names are rejected. Runtime assets and import maps are emitted from the actual accepted runtime imports in the JS source, not inferred from whether a function is fallible. Fallible JS functions may return manual `{ ok: true, value }` / `{ ok: false, error }` wrappers or use imported helpers such as `bstOk` and `bstErr`.

Arbitrary JS dependency graphs, default exports, re-export forms, CommonJS, classes, JS constants, properties/getters/setters, callbacks, async functions, collections/options in JS signatures, generic external types, and multi-success JS returns are deferred.

The HTML builder's first built-in JS-backed package is `@web/canvas`. It is not prelude-imported:

```beanstalk
import @web/canvas

canvas_ref = canvas.get_canvas("game")!
ctx ~= canvas.context_2d(canvas_ref)!
~ctx.set_fill_style("red")
~ctx.fill_rect(0.0, 0.0, 100.0, 100.0)
```

`@web/canvas` exposes opaque `Canvas` and `Canvas2d` types, fallible `get_canvas` and `context_2d` helpers, and a small receiver-method drawing surface: `clear_rect`, `fill_rect`, `set_fill_style`, `begin_path`, `move_to`, `line_to`, and `stroke`.

Prelude external symbols do not override source declarations or explicit imports. Explicit external imports must not collide with already visible source symbols in the same file. External aliases follow the same file-local, collision, and case-convention rules as source import aliases.

Deferred library-system features:
- package manager, package versions, remote fetching, lockfiles, and override/shadowing rules
- source-library HIR caching
- user-authored external binding files
- wildcard imports and direct facade re-export syntax
- automatic docs/API extraction from `#mod.bst`
- seeded random, full date/time/timezone APIs, and Wasm implementations for non-math core packages

### Binding modes and hash (`#`) constants
`#` is a declaration binding-mode marker for compile-time constants. It does not control visibility.

Valid binding forms:

```beanstalk
name = value
name Type = value

name ~= value
name ~Type = value

name #= value
name #Type = value
names #{String} = {"Ana", "Bo"}
maybe_name #String? = none
```

Top-level functions, structs, choices, type aliases, and `#` constants in ordinary files are importable by other files in the same module by default. Cross-module visibility is controlled by the nearest `#mod.bst` facade, whose authored top-level declarations form the public surface. Runtime top-level bindings and expressions are start-body code, not importable declarations.

Template head (`#[...]`) remains entry-file-only top-level const template syntax. It must fully fold at compile time and can contribute compile-time page fragments in builders that use page fragments.

### Top-level declarations and dependency order
Top-level declarations define the module-level declaration surface.
These forms can participate in top-level dependency ordering:

```beanstalk
site_name #= "Beanstalk"

head_defaults #= [:
    <meta charset="UTF-8">
]

UserId as Int

Card = |
    title String,
|

render_card |title String| -> String:
    return [: <article>[title]</article>]
;
```

Top-level constants, type aliases, structs, choices, function signatures, and type annotations can depend on other top-level declarations.

Executable statements inside function bodies do not affect top-level declaration order. Only the module entry file may contain top-level runtime statements.

### Constants and compile-time folding
Constants use the same declaration syntax as regular variables, including optional explicit type annotations.
The difference is semantic: top-level `#` variable declarations are module constants.

Constant rules:
- Must be initialized
- Cannot be mutable
- May only reference constants (non-constant references are compile errors)
- Must fully fold at compile time (runtime expressions are compile errors)
- Same-file constant evaluation follows source order
- Cross-file constant dependencies are resolved in dependency order so constants can reference constants from imported files

Top-level const templates follow the same compile-time rule and are currently entry-file only.

```beanstalk
site_name #String = "Beanstalk"
major_version #Int = 1
full_name #= [: [site_name] v[major_version]]
```

### Struct instances in constants (const records)
Struct instances can be coerced into compile-time records when assigned to a constant.
All constructor arguments must also be constant-foldable values.

```beanstalk
Basic = | defaults String |
values #= Basic("Only allowed const values here")
```

`values` has type `#Basic` and is data-only. Const records do not have a runtime method surface, so `values.some_method()` is not valid.
Const records are field-access-only compile-time member groups. They can be read through fields such as `values.defaults`, and a compile-time constant may alias another const record for facade-style wrappers. The record itself cannot be assigned to a runtime value, passed to a function, returned, or placed in a collection.

```beanstalk
label = values.defaults -- valid
io(values)              -- invalid: use a field
```
