-- Tests accessing error fields inside a catch handler.

can_error |name String| -> String, Float, Error!:
    return! Error("something went wrong")
;

recover |name String| -> String, Float:
    value, score = can_error(name) catch |err|:
        io(err.message)
        then "fallback", 0.0
    ;
    return value, score
;

name, score = recover("test")
io(name)
io(score)
