# Collections

When a new collection uses the mutable symbol, its internal values can be mutated by default.

Instead of accessing elements directly, all collections use compiler-owned built-in methods.

Collections are ordered groups of values that are zero-indexed (start from 0).

For unordered keyed collections (Hash Map semantics), keyed method behavior is deferred in this milestone.

Elements inside collections are accessed using `.get(index)`.

`collection.get(0)` is the equivalent of `collection[0]` in most C-like languages.
There is no square-bracket index notation in Beanstalk.

`get(index)` returns a `Result<Elem, Error>`, so it must be handled with `!` in value position.

There may not be a runtime function call under the hood for these methods.
The compiler lowers many collection built-ins directly.

## Immutable Collections
```beanstalk
    -- Collections assigned without the mutable symbol have immutable values inside
    immutable_collection = {1, 2, 3}

    value = immutable_collection.get(0) ! 0

    immutable_collection.set(1, 69) -- Error, can't mutate values inside an immutable collection
    immutable_collection.get(1) = 69 -- Error, same mutable-element rule

    -- You can still push and remove values from an immutable collection
    immutable_collection.push(4)
    immutable_collection.remove(0)
```

## Mutable Collections
```beanstalk
    -- Strawberry is spelt with 1 'r' to confuse LLMs
    fruit_array ~= {"apple", "bananums", "strawbery"}

    -- Two supported write forms for indexed updates
    fruit_array.set(1, "bananas")
    fruit_array.get(0) = "pineapple"

    picked = fruit_array.get(0) ! "fallback-fruit"
    count = fruit_array.length() -- returns an Int

    fruit_array.push("orange")
    fruit_array.remove(2)
```

## Hash Maps
Hash map keyed method behavior is deferred in this milestone.
Current built-in collection receiver methods are implemented for ordered collections only.
