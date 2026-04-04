-- Tests accessing error fields inside a named handler.

can_error |name String| -> String, Float, Error!:
    return! Error("Parse", "test.field_access", "something went wrong")
;

recover |name String| -> String, Float:
    value, score = can_error(name) err! "fallback", 0.0:
        io(err.message)
    ;
    return value, score
;

name, score = recover("test")
io(name)
io(score)
