prefix = "runtime"

runtime_wrapper = [prefix:
    BEFORE [:
        first slot lives here [..]
        second slot lives here [:
            deeper scene [..]
        ]
    ] AFTER
]

using_runtime_slots = [runtime_wrapper:
    [$1: first]
    in between
    [$2: second]
]

io(using_runtime_slots)
