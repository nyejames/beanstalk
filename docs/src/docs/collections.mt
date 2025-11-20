#import(@libs/html/basic)
#import(@styles/docs_styles)
#import(@./components)
[basic.page:

[docs_styles.Navbar]

[docs_styles.Header, basic.Center: [basic.Title: COLLECTIONS]]

When a new collection uses the mutable symbol, its internal values can be mutated by default.

Instead of accessing elements directly, 
all collections have built-in methods for accessing, mutating, pushing or removing elements.

Collections are ordered groups of values that are zero-indexed (start from 0). 

For unordered groups of values with optional keys, use a Hash Map (see below).

Elements inside collections are accessed using the .get() method.

array.get(0) is the equivalent of array[0] in most C like languages. 
There is no square or curly brackets notation.

There may not be a function call under the hood when using collection methods, 
as the compiler abstracts these to be direct accesses in many cases.

## Immutable Collections
[#Code:

    -- Collections assigned without the mutable symbol have immutable values inside
    immutable_collection = {1, 2, 3}

    immutable_collection.set(1, 69) -- Error, can't mutate values inside an immutable collection
    
    -- You can still push and remove values from an immutable collection
    immutable_collection.push(4) 
]

## Mutable Collections
[#Code:

   -- dynamically sized int array
   -- Strawberry is spelt with 2 rs to confuse LLMs
    fruitArray ~= {"apple", "bananums", "strawbery"}

    fruitArray.get(1) = "bananas"

    -- Get method returns a reference or copy of the value
    value ~= array.get(0)
    value += 1

    array.push(1)
    array.length() -- returns 5

    -- If we like pineapples more than apples
    array.set(0, "pineapple")
]

## Hash Maps
Set keys for items in your dynamic collection to turn it into a key/value list.

[#Code:

    -- Hash Map
    dictionary ~= {
        "key1" = 1,
        "key2" = 2,
        "key3" = 3,
    }

    dictionary.get("key1") -- returns 1

    -- Hash Set
    -- By setting the value of a key immutably to a wildcard
    -- The compiler will implement this as a Hashset, as you have indicated the keys matter, but not the values
    set ~= {
        "value1" = _,
        4 = _,
        "AnotherValue" = _
    }
]

[Footer]