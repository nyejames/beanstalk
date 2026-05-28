import @html {center}
import @styles/docs {navbar, title, section, codeblock, codesnippet, theme_head}

page_title #= "COLLECTIONS"
page_description #= "Introduction to collections in Beanstalk"
page_head #= theme_head

#[navbar]
#[title, center: LANGUAGE BASICS]

#[section, $markdown:
    # Collections

    When a new collection uses the mutable symbol, its internal values can be mutated by default.

    Instead of accessing elements directly, all collections use compiler-owned built-in methods.

    Collections are ordered groups of values that are zero-indexed (start from 0).

    Collection literals are homogeneous. A non-empty collection literal infers its element type
    from its items. Empty collection literals require an explicit collection type annotation
    because their element type is not immediately inferable.

    [codeblock, $code("bst"):
        values ~= {1, 2, 3}      -- inferred as {Int}
        empty_values ~{Int} = {} -- explicit empty Int collection

        values ~= {}             -- Type error: element type is ambiguous
        mixed ~= {1, "bad"}      -- Type error: inconsistent item types
    ]

    The compiler does not infer an empty collection's element type from later `push`, assignment,
    loop, function argument, HIR, or borrow-analysis use. A declaration's type must be explicit at
    the declaration site or immediately inferable from its initializer.

    For unordered keyed collections (Hash Map semantics), keyed method behavior is deferred in this milestone.

    Elements inside collections are accessed using [codesnippet: .get(index)].

    `collection.get(0)` is the equivalent of `collection[0]` in most C-like languages.
    There is no square-bracket index notation in Beanstalk.

    `get(index)` returns an `Elem, Error!` fallible value, so it must be handled in value position.

    `get`, `set`, and `remove` are fallible collection operations. `push` and `length` are
    infallible. Invalid receivers or out-of-bounds indices for fallible operations produce
    structured errors rather than silent no-ops.

    There may not be a runtime function call under the hood for these methods.
    The compiler lowers many collection built-ins directly.

    Use `set(index, value)` for indexed writes. Indexed assignment through `get` has been removed.
]

#[section, $markdown:
    ## Immutable Collections
    [codeblock, $code("bst"):
        -- Collections assigned without the mutable symbol cannot be mutated
        immutable_collection = {1, 2, 3}

        value = immutable_collection.get(0) catch:
            then 0
        ;

        ~immutable_collection.set(1, 69) -- Error, can't mutate values inside an immutable collection
        ~immutable_collection.push(4) -- Error, push mutates the collection
        ~immutable_collection.remove(0) -- Error, remove mutates the collection
    ]
]

#[section, $markdown:
    ## Mutable Collections
    Whenever a method mutates its receiver, the receiver must be marked with the mutable symbol.

    This is also true of built-in collection methods that mutate the collection, such as `set`, `push`, and `remove`.

    [codeblock, $code("bst"):
        -- Strawberry is spelt with 1 'r' to confuse LLMs
        fruit_array ~= {"apple", "bananums", "strawbery"}

        -- Collection must have the mutable symbol to call mutating methods on it
        ~fruit_array.set(1, "bananas") catch:
        ;
        ~fruit_array.set(0, "pineapple") catch:
        ;

        -- Get doesn't need a mutable reference to the collection since it doesn't mutate
        picked = fruit_array.get(0) catch:
            then "fallback-fruit"
        ;
        count = fruit_array.length() -- returns an Int (strictly validated at runtime)

        ~fruit_array.push("orange")
        removed = ~fruit_array.remove(2) catch:
            then "fallback-fruit"
        ;
    ]
]


#[section, $markdown:
    ## Hash Maps
    Hash map keyed method behavior is deferred at this stage of development.
    Current built-in collection receiver methods are implemented for ordered collections only.

    The eventual built-in hash map syntax will look something like this:

    [codeblock, $code("bst"):
        -- Create a new hash map with string keys and int values
        my_map ~= {"key1" = 1, "key2" = 2}

        -- Get an immutable reference to a value from the map
        value = my_map.get("key1") catch:
            then 0
        ;

        -- Set a value in the map (requires mutable reference)
        ~my_map.set("key3", 3) catch:
        ;

        -- Remove a key-value pair from the map (requires mutable reference)
        removed = ~my_map.remove("key2") catch:
            then 0
        ;
    ]
]
