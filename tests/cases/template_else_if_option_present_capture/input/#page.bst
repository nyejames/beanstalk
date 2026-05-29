maybe_name String? = "Ada"

[:template_else_if_option_present_capture
    [if false:
        hidden branch
    [else if maybe_name is |name|]
        hello [name]
    [else]
        hello guest
    ]
]
