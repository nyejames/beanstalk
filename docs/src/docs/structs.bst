[Navbar]

[Header center: [title(1): STRUCTS]]
[Page:

# Structs
Structs are a collection of fields.

They can implement methods by defining a function that takes an instance of the Struct as its first argument.

[#Code:
    -- Define a new object
    -- To create a new instance of this object, it must have 2 parameters passed in,
    -- a string and an integer
    Person:
        name String,
        age Int,
    ;

    -- Create a new instance of the type
    person ~= Person("Alice", 30)

    -- Access fields using dot notation
    print(person.name) -- "Alice"
    print(person.age)  -- 30

    -- Defining a struct, then defining a method for it
    Vector2:
        x Float,
        y Float,
    ;

    reset |vec ~Vector2|:
        vec.x = 0
        vec.y = 0
    ;

    vec = Vector2(12, 87)
    vec.reset()
]

[Footer]