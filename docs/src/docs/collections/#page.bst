import @html {center}
import @styles/docs {navbar, title, section, codeblock, theme_head}

page_title #= "COLLECTIONS"
page_description #= "Introduction to collections in Beanstalk"
page_head #= theme_head

#[navbar]
#[title, center: LANGUAGE BASICS]

#[section, $markdown:
    # Collections

    Collections are ordered groups of values that are zero-indexed (start from 0).
    Instead of accessing elements directly, all collections use compiler-owned built-in methods.

    Collection literals are homogeneous. A non-empty collection literal infers its element type
    from its items. Empty collection literals require an explicit collection type annotation
    because their element type is not immediately inferable.

    [codeblock, $code("bst"):
        values ~= {1, 2, 3}       -- inferred as growable {Int}
        empty_values ~{Int} = {}  -- explicit empty Int collection

        values ~= {}              -- Type error: element type is ambiguous
        mixed ~= {1, "bad"}       -- Type error: inconsistent item types
    ]

    The compiler does not infer an empty collection's element type from later `push`, assignment,
    loop, function argument, HIR, or borrow-analysis use. A declaration's type must be explicit at
    the declaration site or immediately inferable from its initializer.

    Growable collections use `{T}`. Fixed collections use `{N T}`.
    `N` is part of the collection type identity and must be either a positive
    integer literal or a bare visible `#Int` constant name.

    [codeblock, $code("bst"):
        fixed_values {3 Int} = {10, 20}

        capacity #Int = 4
        scratch ~{capacity String} = {}

        larger_capacity #Int = capacity + 2
        larger_scratch ~{larger_capacity String} = {}
        labels {capacity} = {"alpha", "beta"} -- shorthand infers String from the literal
    ]

    Capacity position is not general expression position. Put arithmetic, field access, or other
    computation in a named compile-time constant before using it in a type annotation.

    `{Int}`, `{3 Int}`, and `{4 Int}` are distinct types.
    Fixed capacity is not a growable allocation hint, and Beanstalk does not implicitly convert
    between growable and fixed collections or between different fixed capacities.

    Capacity-only shorthand such as `{capacity}` is valid only for a binding
    declaration with an immediate non-empty collection literal initializer. It is invalid for empty
    literals, non-literal initializers, signatures, aliases, fields, and returns.

    Immutable value bindings cannot start from an empty fixed collection literal because there is
    no later mutable owner path to fill them. Mutable empty fixed bindings and fixed struct field
    defaults are valid.

    Elements inside collections are accessed using ` .get(index)`.

    `collection.get(0)` is the equivalent of `collection[0]` in most C-like languages.
    There is no square-bracket index notation in Beanstalk.

    `get`, `set`, `push`, and `remove` are
    fallible collection operations. `length` is infallible. Invalid receivers,
    out-of-bounds indices, and fixed-capacity overflow produce structured errors rather than
    silent no-ops.

    There may not be a runtime function call under the hood for these methods.
    The compiler lowers many collection built-ins directly.

    Use `set(index, value)` for indexed writes. Indexed assignment through
    `get` has been removed. `set` replaces an existing index only,
    `push` appends after the current last element, and `remove` shifts
    later elements down so dense logical length is preserved.
]

#[section, $markdown:
    ## Immutable Bindings
    [codeblock, $code("bst"):
        -- A collection assigned without the mutable symbol cannot be mutated through this binding.
        read_only_items = {1, 2, 3}

        value = read_only_items.get(0) catch:
            then 0
        ;

        ~read_only_items.set(1, 69) catch:
        ; -- Error, the binding is not mutable

        ~read_only_items.push(4) catch:
        ; -- Error, push mutates the collection

        ~read_only_items.remove(0) catch:
            then 0
        ; -- Error, remove mutates the collection
    ]
]

#[section, $markdown:
    ## Mutable Bindings
    Whenever a method mutates its receiver, the receiver must be marked with the mutable symbol.

    This is also true of built-in collection methods that mutate the collection, such as `set`, `push`, and `remove`.

    [codeblock, $code("bst"):
        -- Strawberry is spelt with 1 'r' to confuse LLMs
        fruit_array ~= {"apple", "bananums", "strawbery"}

        -- Collection must have the mutable symbol to call mutating methods on it
        ~fruit_array.set(1, "bananas") catch:
            assert(false, "This should never fail since the index is in bounds and the collection is mutable")
        ;
        
        ~fruit_array.set(0, "pineapple") catch:
            assert(false, "This should never fail since the index is in bounds and the collection is mutable")
        ;

        -- Get doesn't need a mutable reference to the collection since it doesn't mutate
        picked = fruit_array.get(0) catch:
            then "fallback-fruit"
        ;
        count = fruit_array.length() -- returns an Int (strictly validated at runtime)

        ~fruit_array.push("orange") catch:
            assert(false, "This should never fail for this growable collection in this example")
        ;
        removed = ~fruit_array.remove(2) catch:
            then "fallback-fruit"
        ;
    ]
]

#[section, $markdown:
    ## Fixed Collections
    Fixed collections keep the same dense item behavior as growable collections, but they have an
    exact maximum length in their type.

    [codeblock, $code("bst"):
        items ~{2 Int} = {}

        ~items.push(1) catch:
            assert(false, "First push should fit the fixed capacity")
        ;

        ~items.push(2) catch:
            assert(false, "Second push should fit the fixed capacity")
        ;

        ~items.push(3) catch |err|:
            io(err.message) -- Fixed collection capacity exceeded
        ;

        removed = ~items.remove(0) catch:
            then 0
        ;

        ~items.push(3) catch:
            assert(false, "Remove freed one fixed-capacity slot")
        ;
    ]
]


#[section, $markdown:
    ## Hash Maps
    Hash maps are insertion-ordered key/value collections. Builtin map keys are permanently limited
    to `String`, `Int`, `Bool`, and `Char`.

    [codeblock, $code("bst"):
        scores ~= {"Ada" = 10, "Grace" = 12}
        empty_scores ~{String = Int} = {}

        score = scores.get("Ada") catch:
            then 0
        ;

        ~scores.set("Linus", 7) catch:
        ;

        removed = ~scores.remove("Grace") catch:
            then 0
        ;

        if scores.contains("Ada"):
            io("found")
        ;

        count = scores.length
        ~scores.clear()
    ]

    `get`, `set`, and `remove` are fallible. `contains`, `length`, and `clear` are infallible.
    Mutating map operations require explicit mutable receiver access. More sophisticated maps, such
    as custom-key maps or custom hash/comparison policies, belong in libraries or user-defined
    structs rather than the builtin map syntax.
]
