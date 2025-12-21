import @libs/html/Basic
import @styles/Docs
import @./components
[Basic.page:

[Docs.Navbar]

[Docs.header, Basic.center: [Basic.title(): STRUCTS]]

# Structs
Structs are a collection of fields.

First you define the struct type, then you can create instances of that type.

They can implement methods by defining a function that takes an instance of the Struct as its first argument and has the name 'this'.

```beanstalk
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
    io(person.name) -- "Alice"
    io(person.age)  -- 30

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
```

## Constructor methods
Methods in Beanstalk are created using the special 'this' keyword.

You define a reciever (The type this method will belong to)
by defining a parameter with the special name 'this'.

```beanstalk
    -- Define a struct
    Rectangle:
        width Float,
        height Float,
    ;

    -- Define a method for the Rectangle struct
    area |this ~Rectangle|:
        return this.width * this.height
    ;

    rect = Rectangle(10, 5)
    io(rect.area()) -- 50
```

'this' is a reserved keyword so you can't use it as a variable or field name outside of function parameters and inside the method that uses it.
You are also limited to one 'this' parameter per method.

### Custom Constructors
You can also override the constructor method for a struct by providing an anonymous function as one of the struct's parameters.

```beanstalk
    -- This is how the default constructor would look automatically for this struct
    User:
        | 
            new_name String, 
            new_preferences Preferences,
        |:
            name = new_name
            preferences = new_preferences
        ;

        name String,
        preferences Preferences,
    ;
```

]

[Footer]