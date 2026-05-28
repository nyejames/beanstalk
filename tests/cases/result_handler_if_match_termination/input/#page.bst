can_error || -> String, Error!:
    return! Error("boom")
;

recover || -> String:
    value = can_error() catch |err|:
        if 1 is:
            1 => io("one")
            else => io("other")
        ;
        then "fallback"
    ;

    return value
;

output = recover()
io(output)
