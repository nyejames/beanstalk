# Beanstalk Language Overview

Beanstalk is a programming language and build system for modern UI-driven apps and webpages.

Keep this file focused on compiler-facing language facts: syntax shape, semantic invariants, edge cases, and deferred surface. Put expanded examples, tutorials, and user-facing explanations in docs-site source files under `docs/src/docs/**`.

Design principles:
- Powerful string templates for rendering content and describing UI
- Minimal, consistent syntax that is easy to parse and reason about
- Fast compile times for hot-reload development builds
- Memory safety through GC fallback plus future static ownership optimisation
- Strict static typing with a small number of concise, opinionated patterns

## Related references

- `docs/src/docs/**` — user-facing docs-site pages and real `.bst` examples
- `docs/compiler-design-overview.md` — compiler stage ownership and cross-stage data flow
- `docs/memory-management-design.md` — GC fallback, ownership optimisation, and borrow-analysis strategy
- `docs/src/docs/progress/#page.bst` — current implementation status
- `docs/roadmap/roadmap.md` — planned work

## Syntax Summary

| Feature | Rule |
|---|---|
| Blocks | `:` opens a scope; `;` closes it. Semicolons do not terminate statements. |
| Collections/templates | `{}` are collections. `[]` are string templates only. |
| Comments | `--` starts a single-line comment. |
| Operators | Logical/equality forms use words such as `is` and `not`; symbolic equality and logical-not forms are not operators. |
| Mutability | `~` marks mutable bindings/access. In declarations it appears before the type: `name ~Type = value`. |
| References | Shared immutable access is the default for stack and heap values. |
| Constants | `#` marks compile-time constants: `name #= value`. |
| Facade exports | `export` is reserved everywhere and valid only in `#mod.bst` to mark public facade declarations or grouped re-exports. |
| Copies | Copies must be explicit unless an expression constructs a new value from references. |
| Parameters/fields | Function parameters and struct/choice fields use `|...|`. Defaults use `=`. |
| Results/options | Error returns use `Error!`; options use `T?`. |
| Generics | Declaration-site generics use `type`: `Box type A = | value A |`. Concrete instances use `of`: `Box of String`. |
| Traits | Trait declarations and conformances use `must`; generic bounds use `is`; dynamic trait value annotations use the trait name directly. |
| Renaming | `as` is used for type aliases, namespace import aliases, and grouped import aliases. |
| Shadowing | No visible name may be redeclared while still in scope. |

### Names and shadowing

- Types, structs, choices, generic parameters, and type aliases: `PascalCase`
- Variables and functions: `regular_snake_case`
- Shadowing is invalid: a visible name may not be redeclared in the same visible scope.

Mutable bindings are reassigned with `=`:

```beanstalk
value ~= 1
value = 2 -- reassigns the existing mutable binding
```

## Core Syntax Patterns

```beanstalk
count ~= 0
ratio Float = 1.5
text_slice = "text"
raw_slice = `raw`
letter = '🌱'

message = [:
    Templates create owned strings.
]

values ~{Int} = {}
names {String} = {"Priya", "Gollum"}

Person = |
    name String,
    age Int,
|

Status ::
    Ready,
    Failed | message String |,
;

increment |value Int| -> Int:
    return value + 1
;
 
value = 10
reference = value
copied = copy value

left = "Hello "
right = "world"
joined = [left, right]
```

### Function Calls, Named Arguments, and Mutable Access

Named arguments use `parameter = value`. Access mode is chosen at the call site.

```beanstalk
sum(values)          -- positional shared
sum(~values)         -- positional mutable/exclusive
sum(items = values)  -- named shared
sum(items = ~values) -- named mutable/exclusive
```

Rules:
- Positional arguments must precede named arguments; no positional argument is allowed after the first named argument.
- Each parameter may be supplied once.
- Host calls and builtin member calls are currently positional-only.
- Defaulted parameters may be omitted; named arguments can skip earlier defaults.
- A `~T` parameter accepts `~place` for an existing mutable place, or a plain fresh rvalue such as a literal, template, constructor call, or computed value.
- Passing an existing place to a `~T` parameter without `~` is an error.
- `~` is place-only syntax and is invalid on immutable bindings, literals, templates, constructor calls, and computed expressions.
- Mutable bindings and mutable call access are separate: `value ~= ...` declares or reassigns a mutable binding; `fn(~value)` requests exclusive access for one argument.
- Collections and mutable receiver calls follow the same explicit call-site mutability rule.

Function parameters and struct fields share default-value syntax:

```beanstalk
describe |prefix String = "item", subject String| -> String:
    return prefix + ":" + subject
;

Config = |
    height Int = 100,
    width Int,
|

label = describe(subject = "apple")
default_height = Config(width = 80)
```

Choice payload fields do not currently support defaults.

### Numeric Semantics

| Form | Result |
|---|---|
| Whole-number literal | `Int` |
| Decimal-point literal | `Float` |
| `Int + Int`, `Int - Int`, `Int * Int`, `Int % Int` | `Int` |
| `/` | Real division; `Int / Int -> Float` |
| `//` | Integer division; `Int // Int -> Int`, truncating toward zero |
| Mixed `Int`/`Float` arithmetic | `Float` |

There is no implicit `Float -> Int` coercion. Use `Int(...)` for explicit conversion.

### Options, Results, `then`, and Assertions

#### Options

Optional types use `T?`. `none` requires an optional type context.

```beanstalk
name String? = none

find_name |id String| -> String?:
    if id.is_empty():
        return none
    ;

    return "Alice"
;
```

Rules:
- A `T` value can be used where `T?` is expected.
- `none` is the only special option value.
- Canonical recovery is explicit present-value inspection: `if maybe is |value| then value else fallback`.
- Statement-only absence inspection remains valid: `if maybe is none: ... ;`.
- Full option matches support `none`, literal/relational present-value patterns, `|value|` capture, guards, and `else`.
- Direct fallback syntax such as `maybe else then fallback` is rejected.
- Postfix `?` unwraps a present value or returns `none` from the current function.

```beanstalk
display_name = if maybe_name is |name| then name else "guest"

get_display_name |id String| -> String?:
    user = find_user(id)?
    return user.name
;

name = if maybe_name is |name|:
    then name
else
    then "guest"
;

label = if maybe_name is:
    "Ada" => then "Hi Ada"
    |name| => then name
    none => then "guest"
;
```

#### Error returns

Error-returning functions mark one return slot with `!`. `Error` is builtin and reserved. Its public fields are:
- `message String`
- `code Int = 0`

`ErrorKind`, `ErrorLocation`, and `StackFrame` are ordinary user-available names.

Use `return!` to produce the error path, postfix `!` to bubble it, and `catch:` / `catch |err|:` to recover.

```beanstalk
parse_number |text String| -> Int, Error!:
    if text.is_empty():
        return! Error("Missing number", 200)
    ;

    return 42
;

value = parse_number(text)!

fallback = parse_number(text) catch:
    then 0
;
```

Results are call-site-only. Public first-class `Result` values and result-pattern matching remain deferred. The special `!` return is only for the error path; success values use the normal return list.

#### Value-producing blocks and multi-returns

`then` returns one or more values from the nearest active value-producing block to the receiving site.

Supported producers:
- value-producing `if`
- full match
- block-form `catch:` / `catch |err|:` recovery

Valid receiving sites:
- declarations and assignments
- multi-bind
- returns
- nested `then`

Value-producing blocks are not general expressions and are rejected in function arguments, operator operands, constructor arguments, collection literals, template interpolation, and expression statements.

```beanstalk
value = if condition then 1 else 0
name = if maybe_name is |name| then name else "guest"
fallback = parse_number(text) catch then 0

name, score = load_user(id) catch |err|:
    io(err.message)
    then "guest", 0.0
;
```

Multi-value blocks must produce the receiver arity on every producing path. Multi-bind accepts explicit multi-return function calls and value-producing blocks at closed RHS receiving sites. Regular declarations remain single-target, and user-visible tuple values are not supported.

```beanstalk
pair || -> String, Int:
    return "Ana", 2
;

name, count = pair()
```

#### Assertions

`assert` is a statement-only language intrinsic for invariants.

```beanstalk
assert(index < items.length)
assert(index < items.length, "index must be in bounds")
assert(false, "unimplemented backend path")
```

Rules:
- Assertions are always checked.
- Failure is unrecoverable, does not return `Error!`, and cannot be caught with `catch`.
- `assert` cannot be assigned, passed, imported, aliased, or used in expression position.
- Expected failures should use typed error propagation with `Error!` and `catch`.
- `assert(false)` and `assert(false, "message")` are statically terminal and may end a non-`Void` function or value-required `catch` handler.
- Dynamic `assert(condition)` is not statically terminal.
- Assertion messages are currently string literals only.

### Collections

Collections are ordered, zero-indexed, homogeneous groups.

```beanstalk
values ~= {'a', 'b', 'c'}  -- {Char}
empty_values ~{Int} = {}   -- explicit type required
fixed_values {3 Int} = {10, 20}

capacity #Int = 4
scratch ~{capacity + 2 String} = {}
labels {capacity} = {"alpha", "beta"} -- declaration-target shorthand

items ~= {10, 20, 30}
~items.push(40) catch:
;
~items.set(0, 99) catch:
;
removed = ~items.remove(1) catch:
    then 0
;
```

Rules:
- `{T}` is a growable collection type.
- `{N T}` is a fixed collection type with exact maximum length `N`.
- Fixed capacity is semantic type identity, not an allocation hint: `{Int}`, `{4 Int}`, and `{8 Int}` are distinct incompatible types.
- Capacity expressions in type position must fold to a positive `Int` that fits the compiler target. They may use visible compile-time constants and ordinary compile-time arithmetic.
- `~` is binding/access mode. It is not part of the collection type shape: `~{4 Int}` is mutable access to a `{4 Int}`, not a separate type.
- Non-empty literals infer their element type from items.
- Empty literals require an explicit collection type at the declaration site.
- In a fixed collection receiving context, a literal constructs that fixed collection directly and must not exceed the fixed capacity.
- Capacity-only shorthand such as `{capacity}` is valid only on a binding declaration with an immediate non-empty collection literal initializer that can infer the element type.
- Capacity-only shorthand is invalid for empty literals, non-literal initializers, signatures, aliases, fields, and returns.
- Immutable value bindings cannot be initialized with an empty fixed collection literal. Mutable fixed empty bindings and fixed collection field defaults are valid.
- Element type is not inferred from later `push`, assignment, loop, function-argument, HIR, or borrow-analysis use.
- Mutating operations require explicit mutable/exclusive receiver access: `~items.push(...)`, `~items.set(...)`, `~items.remove(...)`.
- `get`, `set`, `push`, and `remove` are fallible; `length` is infallible.
- `collection.get(index)` returns `Elem, Error!`.
- `~collection.push(value)` returns no success value and must still be handled with `!` or `catch`.
- `~collection.set(index, value)` replaces an existing element only; it does not fill unused fixed capacity.
- `~collection.push(value)` appends after the current last element and fails when a fixed collection is already full.
- `~collection.remove(index)` removes the element at that index, shifts later elements down, and frees one slot in a fixed collection.
- `collection.length()` returns the current logical length, not fixed capacity.
- Indexed writes use `~items.set(index, value)`; assignment through `get` is removed.
- The compiler may lower collection methods directly without a runtime call.

### Standard Output

`io(...)` writes to stdout and accepts strings or templates.

```beanstalk
io("Hello, World!")

name = "Alice"
io([: Hello, [name]])
```

## String Template System

Templates use `[]`; collections use `{}`. `""` creates escaped string slices, and expression-position backticks create raw string slices. Templates create owned strings and may fold at compile time or lower to runtime string construction.

Template head/body shape:

```beanstalk
[$markdown, $children([:<li>[$slot]</li>]):
    # Title
    [: child]
]
```

Core rules:
- The head and body are separated by `:`.
- Authored `.bst` templates must close with `]`; truncated heads, bodies, nested child templates, and directive-argument templates produce syntax diagnostics.
- Template bodies capture variables from the surrounding scope.
- Backticks and Backslashes inside template bodies are ordinary body text (preserved for formatters such as `$markdown`). Regular quoted string literals still support escapes.
- Literal template delimiters in output use ordinary string insertion, such as `[: ["[literal]"]]` or `[: [`[This is text inside sqauare brackets as a string]`]]`.
- Only direct top-level template expressions in an HTML entry file contribute page fragments.
- Top-level runtime templates run in entry `start()` order.
- Top-level const templates fold at compile time and are merged separately.
- Templates assigned to variables or returned from functions do not contribute page fragments by themselves.

### Template Directives

`$` introduces compiler-handled template directives. Directive availability is registry-based:
- Frontend directives are available by default.
- Builders may register project-specific directives with the same `$name` syntax.
- Unknown directives are syntax/rule errors.

| Directive | Meaning |
|---|---|
| `$slot` / `$insert(...)` | Slot declaration/contribution |
| `$fresh` | Opt out of the immediate parent’s `$children(...)` wrapper |
| `$markdown` | Parse body as Beanstalk Markdown |
| `$raw` | Preserve authored body whitespace |
| `$note` / `$todo` | Ignored comments |
| `$doc` | Documentation comment template |
| `$children(...)` | Apply a direct-child wrapper template or string slice |
| `$html` | HTML-builder raw HTML |
| `$css` | HTML-builder CSS checks |
| `$escape_html` | HTML-builder HTML escaping |
| `$code` | HTML-builder code highlighting |

`$children(...)` applies only to direct children. `$fresh` is per-child and only affects wrapper application from the immediate parent. Formatting directives do not flow into nested child templates; redeclare them where needed. For `$children(...)` template arguments, the child template must close with `]` before the directive closes with `)`.

```beanstalk
list #= [$children([:<li>[$slot]</li>]):
    <ul>
        [$slot]
    </ul>
]

[list:
    [: one]
    [$fresh: [: two]]
]
```

### Template Slots

Slots let one template receive content from another.

| Slot form | Meaning |
|---|---|
| `[$slot]` | Default slot |
| `[$slot("name")]` | Named slot |
| `[$slot(1)]`, `[$slot(2)]` | Positional slots |
| `[$insert("name"): ...]` | Contribution to a named slot |

Routing rules:
- Loose contributions fill positional slots first in ascending numeric order.
- Remaining loose contributions go to the default slot if it exists.
- Loose content that cannot be assigned is an error when no default slot exists.
- Missing slots render as empty strings.
- Repeated slots replay the same contribution.
- Runtime-capable templates support default, named, positional, and loose contributions from template `if` and `loop` bodies.
- Runtime slot applications are routed during AST preparation and lower through ordinary runtime string accumulators.

```beanstalk
card = [:
    <h1>[$slot("title")]</h1>
    <section>[$slot]</section>
]

[card:
    [$insert("title"): Hello]
    Body
]

img = [:
    <img src="[$slot(1)]" alt="[$slot]">
]

[img, "logo.png": Site logo]

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

Nested `$children(...)` wrappers remain scoped to direct children, so row/cell-style helpers can be layered without wrapper leakage.

### Template Head Suffix Control Flow

Templates support `if` and `loop` as final head suffixes before the body colon. If the head already has a value, helper, or directive, put a comma before the suffix.

```beanstalk
[if show:
    Visible
]

[card, if show:
    Visible inside card
]

[if maybe_name is |name|:
    Hello [name]
[else if use_fallback]
    Hello fallback
[else]
    Hello guest
]

[loop items |item, index|:
    [if item.skip:
        [continue]
    ]

    [index]: [item]

    [if item.done:
        [break]
    ]
]
```

Template `if`:
- supports `Bool` conditions and option-present capture;
- supports standalone `[else if ...]` with the same selectors;
- supports optional standalone `[else]`;
- does not support full pattern-match branch chains in the template head.

Template `loop`:
- supports conditional loops, collection iteration, and numeric ranges using normal loop-header syntax;
- allows collection/range loops to bind item/counter and optional zero-based index;
- concatenates iterations directly with no implicit separator;
- supports structural `[break]` and `[continue]` targeting the nearest active template loop.

Output/wrapper rules:
- `[head, loop ...:]` wraps the whole aggregate once; put per-iteration wrappers inside the loop body.
- False/no-else branches and zero-iteration loops produce structural no-output and skip shared wrappers.
- Output before `[break]` or `[continue]` is preserved; iterations with no output before control flow do not count as emitted.

Const rules:
- Top-level `#[if ...:]` and `#[loop ...:]` must fully fold at compile time.
- Const-required `if` validates every branch body.
- Const conditional loops fold to no-output only when the condition is compile-time `false`; compile-time `true` and runtime/unknown conditions are rejected.
- Const range/collection loops use `template_const_loop_iteration_limit`, default `10_000`, capped at `1_000_000`.
- Runtime template control flow is lazy.

Runtime slot applications are valid inside template control flow after normal slot routing. Escaped unresolved `[$slot]` or `$insert(...)` artifacts inside runtime control-flow bodies are invalid template structure. Runtime slot applications appended inside template loops follow the same nearest-loop `[break]` / `[continue]` rules as direct loop body content.

### Markdown Formatting

`$markdown` is Beanstalk's small markdown flavour, not a full CommonMark implementation.

Inline code uses paired isolated single backticks on the same markdown line and renders as `<code>...</code>`.
Markdown emphasis and link parsing do not run inside inline code. Empty spans, repeated backtick runs, unmatched backticks, multiline spans, variable-length delimiters, fenced code blocks, and markdown-level backtick escaping are not part of Beanstalk's markdown flavour.

Dynamic expression anchors may appear inside a parent-authored code span, but `$markdown` does not inspect their rendered output. Child templates are opaque barriers to the parent formatter and cannot be inside a parent-authored code span or pair delimiters across that child boundary.

### Beandown `.bd` Content Files

Beandown files are HTML-builder content helpers. A `.bd` file is authored as the body of an implicit compile-time markdown template:

```beanstalk
content #String = [$markdown:
    ...entire .bd file body...
]
```

The compiler builds that structure directly; it does not prepend wrapper source text.

Import `.bd` files with normal extensionless source import syntax:

```beanstalk
import @docs/intro
import @docs/intro {
    content as intro_content,
}

[:[intro.content] [intro_content]]
```

Rules:
- A `.bd` file exposes exactly one generated constant, `content #String`.
- Direct extension imports such as `import @docs/intro.bd` are invalid; use `import @docs/intro`.
- `.bd` files are never page entries, module roots, config files, or standalone project types.
- `.bd` files have no imports, declarations, frontmatter, metadata, or raw-source preservation.
- A `.bd` body must fully fold at compile time. Runtime functions, runtime bindings, and types are not visible.
- The implicit markdown template means `--` is body text, not a Beanstalk comment.
- `.bd` bodies follow normal template-body and `$markdown` semantics. Literal template delimiters use string insertion, such as `["[literal]"]`; nested authored templates still use normal `[...]` syntax and must close explicitly.

Inside compiler-integrated HTML project builds, a `.bd` body sees a restricted flat compile-time scope:
- exported compile-time constants and const records from `@html`, such as `[p: body]`;
- exported compile-time constants and const records from the same-directory `#mod.bst`, when one exists.

Same-directory facade constants override `@html` constants on name collision. Functions, structs, choices, type aliases, traits, methods, external/runtime APIs, and the generated self `content` constant are not visible in the `.bd` body.

Facades can re-export Beandown content explicitly:

```beanstalk
-- src/#mod.bst
import @core/text {length as private_length}

export page_label #= "Documentation"

export title_length |title String| -> Int:
    return private_length(title)
;

export @components/card {
    CardData as Card,
    render_card,
}
```

Use `.bst` files for pages, composition, imports, functions, and richer compile-time setup. Use `.bd` for small markdown-first content fragments consumed by `.bst`.

### If Statements and Pattern Matching

Statement `if` is non-exhaustive. It has no statement-level `else if`; use nested `if` or full match.

```beanstalk
if value is true:
    io("then")
else
    io("else")
;
```

Full pattern matching uses `if <value> is:` and is exhaustive.

```beanstalk
if value is:
    < 0 => io("negative")
    0 => io("zero")
    <= 10 => io("small")
    else => io("large")
;
```

Rules:
- Arms are delimited by the next line-initial arm, `else =>`, or the final `;`.
- Per-arm semicolons are invalid.
- Arm headers must start at the beginning of a logical line.
- Arm forms are `<pattern> => <body>`, `<pattern> if <Bool> => <body>`, `else => <body>`, and bodyless `else =>`.
- In statement matches, bodyless `else =>` catches all remaining cases and executes no statements.
- Non-choice scrutinees require `else =>`.
- Choice scrutinees require either `else =>` or coverage of every variant.
- Any guarded choice arm requires `else =>`.
- The same choice variant cannot be matched more than once.
- The catch-all default arm is only `else =>`. `_ => ...` is not a wildcard pattern.
- `else => _` is invalid; use bodyless `else =>` for an explicit no-op fallback.

A statement match can use an empty fallback arm to explicitly ignore all remaining cases:

```beanstalk
if value is:
    0 => io("zero")
    else =>
;
```

Empty `else =>` is for statement matches. Value-producing matches must still produce the required `then` values on every selected path.

Supported patterns:
- literals: `1`, `"ok"`, `true`
- choice variants: `Ready`, `Status::Ready`
- choice payload captures: `Err(message)`, `Pending(retry_count, message)`
- renamed payload captures: `Err(message as error_text)`
- general capture: `captured` binds the matched scrutinee value for that arm
- relational scalar patterns: `< 0`, `<= 10`, `> 0`, `>= 100`

Payload capture names must match declared field names unless renamed with `as`. Payload exhaustiveness is tag-level. Relational patterns support ordered scalar scrutinees: `Int`, `Float`, `Char`, and `String`; string ordering is backend-defined in Alpha. Nested choice payload patterns are deferred.

## Loops

Beanstalk has one loop keyword: `loop`.

```beanstalk
loop condition:
    ...
;

loop items |item, index|:
    ...
;

loop 0 to 10 by 2 |i|:
    ...
;
```

Forms:
- conditional loop: repeats while a `Bool` condition is true;
- collection loop: yields item and optional zero-based index;
- range loop: yields counter and optional zero-based index.

Range rules:
- `to` is exclusive; `to &` is inclusive.
- `loop to n` is sugar for `loop 0 to n`.
- Direction is inferred from bounds.
- Without `by`, ascending ranges use `+1`; descending ranges use `-1`.
- With descending bounds, `by` is treated as a magnitude and the compiler applies the sign.
- `by 0` is invalid.
- Float ranges are supported, but `by` should be treated as required to avoid ambiguous or non-terminating loops.
- Bindings use `|...|` after the loop source and may be omitted when unused.

```beanstalk
loop 0.0 to 1.0 by 0.1 |t|:
    io([t])
;
```

## Structs and Receiver Methods

Structs are nominal runtime types. Matching field shapes do not make two structs interchangeable. Type aliases to structs are transparent and do not create new struct identity.

```beanstalk
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

Receiver method rules:
- A receiver method is a top-level function whose first parameter is named `this`.
- `this` is reserved and may appear only as the first receiver parameter and inside that method body.
- There may be exactly one `this` parameter.
- Supported receiver types are user-defined structs, choices, aligned declaration-site generic nominal receivers, built-in scalars (`Int`, `Float`, `Bool`, `String`), imported source nominal types for file-local extensions, and exact external opaque types exposed by backend package metadata.
- Collection built-ins are compiler-owned operations, not `this` methods.
- `this T` is immutable; `this ~T` is mutable.
- Mutable receiver calls require explicit mutable/exclusive receiver syntax: `~value.method(...)`.
- Receiver methods are called only with receiver syntax; `method(value, ...)` is invalid.
- Mutable receiver methods require a mutable place receiver; temporaries and rvalues cannot be mutated.
- Field writes follow the same mutable-place rule.
- User-defined struct methods must be declared in the same file as the struct. Built-in scalar receiver methods are exempt.

Receiver method visibility:
- Exported receiver methods become available through the receiver type, not as free-function imports.
- Namespace imports make receiver methods available through receiver-call syntax but not as namespace fields.
- Receiver-method names can be shared by receiver methods because dispatch includes receiver type, but they still collide with ordinary value, type, namespace, prelude, and builtin names.
- Importing a receiver type automatically imports receiver methods for that type from the same import surface.
- Grouped imports may import/alias receiver methods, but the receiver type must already be visible through the same grouped import, an earlier type import, a namespace import from that surface, or an exact transparent type alias.
- JS-backed external receiver methods use the exact package-scoped opaque receiver type declared by `@bst.opaque`.
- Type aliases in a `#mod.bst` facade do not automatically re-export private implementation methods.

```beanstalk
import @web/canvas { Canvas2d, fill_rect }
import @web/canvas { fill_rect } -- invalid unless Canvas2d is visible
```

## Traits

Traits are explicit nominal method contracts.

```beanstalk
DISPLAY_TEXT must:
    display |This| -> String
;

Label = |
    text String,
|

display |this Label| -> String:
    return this.text
;

Label must DISPLAY_TEXT
```

Rules:
- Trait names use all-caps identifiers.
- `TRAIT must:` declares a top-level trait contract.
- Trait requirements are method signatures only; marker traits with no requirements are valid.
- Requirement receivers use `This` or `~This`, not lowercase `this`.
- Bare `This` and `~This` are receiver-only and valid only as the first requirement parameter.
- Direct non-receiver `This` parameters must be named, for example `other This`.
- Direct return `This` is supported.
- Composed `This` forms such as `This?`, `{This}`, and `Box of This` are rejected in the Alpha surface.
- `This` is trait-local syntax and is rejected outside trait declarations.
- `Type must TRAIT` declares explicit conformance. It is bodyless and newline-terminated; do not add a semicolon.
- A matching method without `Type must TRAIT` is not conformance.
- Conformance validates exact receiver mutability, non-receiver parameter modes/types, return types, and return channels. Parameter names do not matter.
- Canonical conformance evidence for same-file structs, choices, and generic type constructors is reusable wherever both the type and trait are visible.
- File-local extension evidence for builtins, imported types, and external opaque types is usable only in the declaring file and cannot override visible canonical evidence.

A trait name in a normal type annotation means a dynamic trait value.
A trait name in a generic bound constrains a concrete generic parameter.
These are different features.

```beanstalk
render_dynamic |value DISPLAY_TEXT| -> String:
    return value.display()
;

render_static type Item is DISPLAY_TEXT |value Item| -> String:
    return value.display()
;
```

Dynamic trait values are opaque owning wrappers. Concrete values coerce to dynamic trait values only at explicit typed boundaries: annotated declarations, function arguments, returns, struct fields, choice payloads, and explicitly typed collection elements. Unannotated locals and collection literals do not infer trait values.

All traits can be generic bounds. Only dynamic-safe traits can be value types. A trait is not dynamic-safe if a requirement returns `This`, takes `This` outside the receiver, or otherwise requires recovering the erased concrete identity.

Dynamic trait runtime lowering is supported by the JavaScript backend. HTML-Wasm rejects reachable dynamic trait construction and dispatch with structured unsupported-backend diagnostics; unreachable dynamic-only helper functions are ignored by that reachability validation. Static trait declarations, conformances, and generic bounds remain frontend semantics and are backend-independent.

Deferred trait surfaces include default methods, associated types/constants, static non-method requirements, inheritance, generic traits, generic trait methods, specialized or conditional conformance, dynamic trait aliases/composition/downcasting, file-local evidence-backed generic bound dispatch, automatic primitive conformances, Wasm dynamic trait runtime lowering, `DISPLAYABLE` output coercion, and operator-to-trait integration.

## Choices

Choices are nominal tagged unions. Variants are either unit variants or record-payload variants.

```beanstalk
Result ::
    Ok,
    Err | message String, code Int |,
;

success = Result::Ok
failure = Result::Err(message = "bad request", code = 400)
```

Rules:
- Unit variants are constructed as `Choice::Variant`.
- Payload variants are constructed as `Choice::Variant(...)` with positional or named arguments.
- Payload fields are immutable after construction.
- Direct payload field access is deferred and should use pattern matching.
- Payload field mutation is rejected.

Structural equality is supported only when every payload field across every variant supports equality.

Supported equality payloads:
- `Int`, `Float`, `Bool`, `Char`, `String`
- choices whose payloads all support equality
- options whose inner type supports equality

Unsupported equality payloads:
- structs
- collections
- functions
- fallible result carriers
- external opaque types
- templates

## Generics

Generics are declaration-site type abstractions for top-level structs, choices, and free functions. Generic parameters are introduced with `type` after the declaration name.

```beanstalk
identity type A |value A| -> A:
    return value
;

Box type A = |
    value A,
|

Maybe type A ::
    Some | value A |,
    None,
;
```

Generic parameter rules:
- Names use type-name style.
- Parameters are scoped to the declaration.
- Parameters are compile-time placeholders, not runtime values.
- A parameter cannot collide with another parameter in the same declaration or with visible concrete types, type aliases, external package types, builtins, or other type-position names.

Generic function calls use normal call syntax only. Type arguments are inferred from immediate call arguments and, at closed receiving sites, the immediate expected result type.

```beanstalk
value = identity(42)
typed_value Int = identity(42)

empty type A || -> {A}:
    return {}
;

items {Int} = empty()
```

Inference does not use later mutation, later use, whole-program analysis, HIR, borrow validation, or expected parameter context from an outer function call into a nested generic call. Use ordinary annotations to make nested calls explicit.

Unconstrained generic code can pass values through, return them, store them in generic structs/choices, forward them to other generic functions when immediate call evidence solves the parameters, and use generic parameters in local annotations.

Declaration-site trait bounds use `is`:

```beanstalk
render type Item is DISPLAY_TEXT |item Item| -> String:
    return item.display()
;

render_pair type A is DISPLAY_TEXT, B is DISPLAY_TEXT |left A, right B| -> String:
    return left.display() + right.display()
;
```

Use `and` for multiple bounds on one parameter. Commas still separate generic parameters. `where` syntax remains rejected. Concrete generic calls and generic struct/choice instantiations require visible reusable evidence for each concrete type argument. Dynamic trait values do not satisfy static generic bounds.

Operations that require behavior from an unconstrained generic type are rejected. Trait bounds currently enable unique bound-provided receiver calls. Arithmetic, equality/comparison, field access, template interpolation requiring string-like behavior, and external/IO behavior still require concrete type support or a future dedicated trait integration.

Concrete generic aliases are supported:

```beanstalk
StringBox as Box of String
```

Name intermediate concrete aliases instead of writing nested `of` applications:

```beanstalk
Pair type A, B = |
    first A,
    second B,
|

StringIntPair as Pair of String, Int
value Box of StringIntPair = Box(Pair("count", 3))
```

Rejected or deferred in the current Alpha surface:
- explicit call-site syntax such as `identity of Int(42)`, `identity<Int>(42)`, `identity[Int](42)`, or `identity(42 Int)`
- inline generic sugar such as `|value type A|`
- generic function values and higher-order polymorphism
- type values, type-returning functions, type-level `#if`, and compile-time type inspection
- receiver methods on concrete generic instances
- `where` clauses and file-local evidence-backed generic bound dispatch
- generic external package functions and generic external package types
- recursive generic types
- nested `of` applications except through concrete alias workarounds
- parameterized generic aliases and partial type application

## Type Aliases

Type aliases give another compile-time name to an existing type. They are transparent and do not create nominal identity.

```beanstalk
UserId as Int
Names as {String}
MaybeName as String?
StringBox as Box of String

import @types { UserId as ExternalId }
LocalId as ExternalId

id UserId = 42
raw Int = id -- valid
```

Aliases can target builtins, structs, choices, options, collections, fully concrete generic instances, imported types, and external package types.

## Module System, Config, and Imports

A module is a directory-scoped set of Beanstalk source files compiled into one output. A directory becomes a module root when it contains one or more `#*.bst` files, excluding `#config.bst`. A project contains one or more modules plus libraries and other builder inputs.

### Project config

`#config.bst`:
- lives at the project root;
- uses normal declaration syntax;
- accepts only known config-key value declarations;
- requires values to fold at compile time;
- allows plain immutable key declarations and explicit `#` key constants;
- may reference earlier compile-time config keys or constants imported from core/builder source libraries;
- may contain core/builder imports, type aliases, structs, and choices as support declarations;
- rejects project-local/relative imports, mutable bindings, functions, calls, host calls, runtime statements, non-key helper constants, standalone templates, and `#[...]` page fragments.

Known config key shapes include:
- string settings: string literals or folded templates;
- `project`: currently only `"html"`;
- boolean HTML settings: folded `Bool`, not strings;
- `library_folders`: one string or a collection of strings;
- `template_const_loop_iteration_limit`: positive folded `Int`, default `10_000`, max `1_000_000`.

```beanstalk
project = "html"
entry_root = "src"
dev_folder = "dev"
output_folder = "release"
library_folders = {"lib", "packages"}
```

Explicit `#` config-key constants can use const-record field projection when the expression fully folds, for example `entry_root #= Defaults().entry_root`. The same projection in a plain `=` config key is deferred. Structured typed config values such as `project = Project::Html(...)` remain deferred.

### Import syntax and rules

```beanstalk
import @path/to/file
import @core/math as math
import @vendor/drawing.js as drawing

import @path/to/file {symbol}
import @path/to/file {symbol as local_name}

import @components {
    render as render_component,
    Button as UiButton,
    Card,
}

import @docs {
    pages/home {render as render_home},
    pages/about {render as render_about},
}
```

Rules:
- Imports target exported symbols, not file-level start functions.
- Namespace imports create shallow, field-access-only import records.
- Import records are not first-class values.
- Import child paths directly; do not traverse nested path segments through namespace fields.
- Aliases are file-local, not re-exported, and must not collide with visible names.
- Alias leading-case mismatches warn.
- Grouped imports cannot use a trailing group-level alias.
- Direct symbol-path imports such as `import @core/math/sin` are invalid.
- `.bst` source imports are extensionless.
- Direct project/local JavaScript imports require `.js` and a builder `.js` external import provider.
- Invalid namespace path stems require explicit aliases.
- Direct imports of `#mod.bst`, `#page.bst`, and `#config.bst` are invalid.

### Module roots, entry files, facades, and libraries

| File/root | Role |
|---|---|
| `#page.bst` or other builder entry | Owns top-level runtime/start code |
| implicit `start` | Contains entry-file top-level runtime code; build-system-only; not importable |
| non-entry `.bst` files | Declarations only; no top-level executable statements |
| `#mod.bst` | Only outward-facing module export surface |
| module root without `#mod.bst` | Exports nothing outside itself |
| `#config.bst` | Affects build behavior; creates no language-visible imports |

Execution and visibility:
- Only the module entry file executes top-level runtime code automatically.
- Other files contribute declarations that must be imported explicitly.
- `#page.bst` may import same-module files but exports nothing unless `#mod.bst` exposes declarations.
- `#mod.bst` is an API facade, not a runtime entry or shared implementation file.
- `#mod.bst` may contain private imports plus private or public authored top-level functions, structs, choices, type aliases, traits, and `#` constants.
- `#mod.bst` may not contain top-level runtime statements, runtime templates/start code, or `#[...]` page fragments.
- Public authored facade declarations require `export`.
- Regular `import` in `#mod.bst` is private to that facade file.
- `export import @path { Symbol }` and `export @path { Symbol }` re-export imported symbols through the facade; grouped aliases define the public API name.
- Public facade APIs must not expose private facade-only types in signatures, fields, aliases, generic bounds, or exported constant types.
- Receiver methods are visible through a facade only when the receiver method is explicitly exported by the same facade surface; type aliases in `#mod.bst` do not automatically re-export private implementation methods.
- Bare namespace exports such as `export @path`, wildcard exports, legacy `#import`, and function alias exports are not part of the Alpha surface.

Import resolution:
- `@./x` resolves from the importing file’s directory.
- Parent-directory imports with `..` are unsupported.
- Imports cannot escape module/library/project boundaries.
- Library-prefix imports resolve from the matching library root.
- Other non-relative imports resolve from the configured module entry root.
- Configured library folders are scan roots; each direct child directory becomes an import prefix.
- `/lib` is the default scan folder when `library_folders` is omitted.
- Importing a folder across or into a module boundary requires that folder to expose `#mod.bst`.
- Sibling `name.bst` and `name/` folder imports are ambiguous; `.js` files are excluded from that collision rule.
- Grouped imports expand into individual symbol imports.
- Circular imports are compilation errors.

Library categories:
- core prelude libraries: every builder must provide `@core/prelude`; exported prelude names are bare;
- core libraries: optional builder packages such as `@core/math`, `@core/text`, `@core/random`, `@core/time`;
- builder libraries: builder-owned libraries such as HTML `@html`;
- project libraries: source libraries discovered from configured library folders;
- external packages: virtual packages provided by backend metadata.

Core libraries require explicit imports unless they are part of the prelude. Unsupported builder packages are rejected with an unsupported-by-builder diagnostic. Source libraries are normal modules behind `#mod.bst` facades.

The HTML builder's `@html` source library exposes authored HTML helpers, including `canvas`, `CANVAS_ID`, and `get_canvas_context`. Those are real facade declarations backed by local `@web/canvas` imports inside `libraries/html/#mod.bst`. The raw `@web/canvas` symbols themselves are not re-exported through `@html`. Import raw drawing APIs directly from `@web/canvas` when needed.

### External platform package imports

Project builders may provide virtual packages such as `@core/io`, `@core/math`, or `@web/canvas`. These are typed external packages, not Beanstalk source files. They expose external functions and opaque external types.

```beanstalk
import @core/math
import @core/math { sin as sine }

value = math.sin(1.0)
aliased = sine(1.0)
```

Rules:
- The implementation matrix is the source of truth for supported external packages and backend targets.
- For normal builds, `io()`, `IO`, and compiler-owned `Error` are available without explicit imports.
- Prelude external symbols do not override source declarations or explicit imports.
- Explicit external imports must not collide with visible source symbols.
- External aliases follow normal file-local collision and case-convention rules.
- External opaque types can be passed, returned, and used by external functions, but cannot be constructed with struct syntax or field-accessed.

Initial optional core packages:
- `@core/math`: `PI`, `TAU`, `E`, and `Float` math helpers.
- `@core/text`: `length`, `is_empty`, `contains`, `starts_with`, `ends_with`.
- `@core/random`: `random_float`, `random_int`; `random_int(min, max)` is inclusive at both ends and swaps bounds when `min > max`; seeded random is deferred.
- `@core/time`: opaque `Duration`, `TimeMark`, and `Timestamp` types; monotonic `mark_now`, `elapsed_since`, and `duration_between`; duration construction/conversion helpers; Unix timestamp construction/conversion helpers; and fallible ISO timestamp parsing/formatting.

Time package split:
- Use `TimeMark` for elapsed time and frame deltas.
- Use `Timestamp` for real-world UTC instants.
- Use `Duration` for elapsed amounts.
- `timestamp_from_iso_string` is fallible and must be handled with postfix `!` or `catch`.

The HTML builder supports annotated single-file `.js` imports through `@bst.opaque` and `@bst.sig`. JavaScript export names are runtime implementation details; Beanstalk names come from annotations. Supported JS export forms are `export function name(...) { ... }` and block-bodied arrow exports. Runtime imports from builder-registered modules must be named static imports. Unsupported JS features include arbitrary dependency graphs, default exports, re-exports, CommonJS, classes, JS constants, property accessors, callbacks, async functions, collections/options in JS signatures, generic external types, and multi-success JS returns.

Deferred library-system features:
- package manager, versions, remote fetching, lockfiles, and override/shadowing rules
- source-library HIR caching
- user-authored external binding files
- wildcard imports/exports and namespace facade exports such as `export @path`
- automatic docs/API extraction from `#mod.bst`
- seeded random, full date/time/time-zone/calendar APIs, Temporal-backed calendar implementation, locale-aware formatting/parsing, local time-zone lookup, async timers/sleep/intervals, browser animation scheduling packages, and non-JS lowerings for JS-backed core packages

### Binding Modes, Top-Level Declarations, and Constants

Binding forms:

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

Rules:
- `#` marks compile-time constants; it does not control visibility.
- Top-level functions, structs, choices, type aliases, and `#` constants in ordinary files are importable inside the same module by default.
- Cross-module visibility is controlled by the nearest `#mod.bst` facade.
- Runtime top-level bindings and expressions are start-body code, not importable declarations.
- `#[...]` is entry-file-only top-level const-template syntax; it must fully fold and may contribute compile-time page fragments.

Top-level dependency ordering includes constants, type aliases, structs, choices, function signatures, and type annotations. Executable body statements do not affect top-level declaration order.

Constant rules:
- Must be initialized.
- Cannot be mutable.
- May reference only constants.
- Must fully fold at compile time.
- Same-file constant evaluation follows source order.
- Cross-file constant dependencies are resolved in dependency order.

```beanstalk
site_name #String = "Beanstalk"
major_version #Int = 1
full_name #= [: [site_name] v[major_version]]
```

Struct instances assigned to constants can become data-only const records when all constructor arguments fold. Const records are field-access-only compile-time member groups. They cannot be assigned to runtime values, passed, returned, placed in collections, or used through runtime methods.

```beanstalk
Basic = | defaults String |
values #= Basic("Only allowed const values here")

label = values.defaults -- valid
io(values)              -- invalid: use a field
```
