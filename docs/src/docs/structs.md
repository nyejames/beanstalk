# The Struct
A struct is a collection of named fields, similar to a struct in other languages.

Runtime structs are nominal types. Two structs with the same field shape are still different runtime types.

They can implement statically resolved receiver methods by defining a top-level function whose first argument is literally named `this`.
`this Type` is immutable, `this ~Type` is mutable, and receiver methods can only be called with `value.method(...)`.
User-defined struct methods must be declared in the same file as the struct definition.

Receiver methods are also supported on built-in scalar types (`Int`, `Float`, `Bool`, `String`) and use the same `this` rules. The same-file restriction applies to user-defined struct methods.

Structs can be assigned with default arguments, these can be any constant expression.
```beanstalk
    Person = |
        name String,
        age Int,
    |

    -- Create a new instance of the type
    person ~= Person("Alice", 30)

    -- Access fields using dot notation
    io(person.name) -- "Alice"
    io(person.age)  -- 30

    -- Defining a struct, then defining a method for it
    Vector2 = |
        x Float = 0,
        y Float = 0,
    |

    reset |this ~Vector2|:
        this.x = 0
        this.y = 0
    ;

    vec = Vector2(12, 87)
    ~vec.reset()
```

## Methods
Methods in Beanstalk are created using the special `this` receiver parameter.

You define a receiver by making the first parameter literally named `this`.

```beanstalk
    -- Define a struct
    Rectangle = |
        width Float,
        height Float,
    |

    -- Define an immutable method for the Rectangle struct
    area |this Rectangle|:
        return this.width * this.height
    ;

    rect = Rectangle(10, 5)
    io(rect.area()) -- 50
```

`this` is reserved for method receivers. You are limited to one `this` parameter per method, and `method(value, ...)` is not valid syntax for calling receiver methods.

Const-coerced struct values are data-only records. They can be read, but they do not expose runtime methods.