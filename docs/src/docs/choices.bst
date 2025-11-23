#import(@libs/html/Basic)
#import(@styles/Docs)
#import(@./components)
[Basic.page:
[Docs.navbar]

[Docs.header Basic.center: [title(1): CHOICES]]
[Basic.page:

# Choices (enums)
Choices are a way to define a set of possible values. They are similar to enums in Rust (tagged unions).

Choices use a double colon for their declaration.

You access one of the choices using double colon also.

```beanstalk
    Choice ::
        Option1 String,
        Option2 String,
    ;

    option = Choice::Option2
```

[Docs.footer]
