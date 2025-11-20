#import(@libs/html/basic)
#import(@styles/docs_styles)
#import(@./components)
[basic.page:

[docs_styles.Navbar]

[docs_styles.Header, basic.Center: [basic.Title: STRUCTS]]

# Structs
Structs are a collection of fields.

They can implement methods by defining a function that takes an instance of the Struct as its first argument.

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
```

## Constructor methods
You can explicitly define a default method for a struct by providing an anonymous function as one of the struct's parameters.

```beanstalk
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



[Footer]