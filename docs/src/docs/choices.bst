#import(@libs/html/basic)
#import(@styles/docs_styles)
#import(@./components)
[basic.page:
[Navbar]

[Header center: [title(1): CHOICES]]
[Page:

# Choices (enums)
Choices are a way to define a set of possible values. They are similar to enums in Rust (tagged unions).

Choices use a double colon for their declaration.

You access one of the choices using double colon also.

[#Code:
    Some_Options Choice:
        Option1 String,
        Option2 String,
    ;

    option = Choice::Option2
]

[Footer]