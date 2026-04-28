# Choices (enums)
Choices are a way to define a set of possible values. 
The Alpha compiler supports unit variants and record-body payload variants.
Generic choices, recursive choices, direct payload field access, nested payload patterns, structural equality for payload variants, and default values are deferred.

You create and access choices using a double colon.

```beanstalk
    #Status :: Ready, Busy, Offline;

    current Status = Status::Busy

    pass_status |status Status| -> Status:
        return status
    ;

    selected = pass_status(current)
```

Choices work with assignment and pattern matching.

```beanstalk
    #Status :: Ready, Busy;

    current ~= Status::Ready
    current = Status::Busy

    label ~= "unset"
    if current is:
        case Ready => label = "ready"
        case Busy => label = "busy"
    ;
```

Payload variants use record-body syntax and support constructor calls.

```beanstalk
    Response ::
        Success,
        Error |
            message String,
        |,
    ;

    error = Response::Error("bad")
```

Payload fields are extracted through pattern matching. Captures use the declared field names and may be renamed with `as`.

```beanstalk
    if error is:
        case Success => io("done")
        case Error(message as error_message) => io(error_message)
    ;
```

These richer choice forms are reserved for post-Alpha design work and currently reject with structured diagnostics:

```beanstalk
    -- Payload shorthand: deferred.
    Response :: Error String, Success;

    -- Unit variants are values, not empty constructor calls.
    value = Response::Success()
```

Defaulted choice variants are also deferred.

```beanstalk
    Status ::
        Active Bool = true,
        Inactive Bool = false,
    ;

    -- No need to set a value, it defaults to 'true'
    currentStatus = Status::Active
```
