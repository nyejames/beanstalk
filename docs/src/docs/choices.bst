import @libs/html/Basic
import @styles/Docs
import @./components
[Docs.navbar]
[Basic.page:

[Docs.header Basic.center: [title(1): CHOICES]]
[Basic.page:

# Choices (enums)
Choices are a way to define a set of possible values. They are similar to enums in Rust (tagged unions).

You access one of the choices using double colon.

```beanstalk
    Choice ::
        Option1,
        Option2,
        Option3,
    ;

    option = Choice::Option2

    -- Another example with data attached to the Error choice
    ResponseKind ::
        Success,
        Error String,
        Pending,
    ;

    response = ResponseKind::Error("Network failure")
```

Choices can also be defined with default values, the same way structs can.

```beanstalk
    Status ::
        Active = true,
        Inactive = false,
    ;

    -- No need to set a value, it defaults to 'true'
    currentStatus = Status::Active
```

[Docs.footer]
