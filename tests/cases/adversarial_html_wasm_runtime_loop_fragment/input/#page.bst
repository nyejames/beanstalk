value ~= 0

loop i in 0 to 3:
    value = value + i
;

message = [:
    wasm value [value]
]

io(message)
[:<section>[message]</section>]
