can_error |name String| -> String, Float, Error!:
    return! Error("boom")
;

recover |name String| -> String, Float:
    value, score = can_error(name) catch |err|:
        io(err.message)
        then "guest", 0.0
    ;
    return value, score
;

name, score = recover("Ana")
io(name)
io(score)
