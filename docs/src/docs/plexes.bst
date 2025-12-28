import @libs/html/Basic
import @styles/Docs
import @./components
[Basic.page:

[Docs.Navbar]

[Docs.header, Basic.center: [Basic.title(): PLEXES]]

# The Plex
A plex is a collection of named fields, similar to a struct in other languages.

You define the shape of a Plex using the || parameter brackets syntax.

They can implement methods by defining a function that takes an instance of the Plex as its first argument with the name 'this'.

Plexes can be assigned with default arguments, these can be any constant expression.

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

    -- Defining a plex, then defining a method for it
    Vector2 = |
        x Float = 0,
        y Float = 0,
    |

    reset |this ~Vector2|:
        this.x = 0
        this.y = 0
    ;

    vec = Vector2(12, 87)
    vec.reset()
```

## Methods
Methods in Beanstalk are created using the special 'this' keyword.

You define a reciever (The type this method will belong to)
by defining a parameter with the special name 'this'.

```beanstalk
    -- Define a plex
    Rectangle = |
        width Float,
        height Float,
    |

    -- Define a method for the Rectangle plex
    area |this ~Rectangle|:
        return this.width * this.height
    ;

    rect = Rectangle(10, 5)
    io(rect.area()) -- 50
```

'this' is a reserved keyword so you can't use it as a variable or field name outside of function parameters and inside the method that uses it.
You are also limited to one 'this' parameter per method.

]

[Footer]