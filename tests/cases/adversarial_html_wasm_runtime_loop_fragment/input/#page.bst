value ~= 0

loop 0 to 3 |i|:
    value = value + i
;

message = [:
    wasm value [value]
]

io(message)
[:<section>[message]</section>]
