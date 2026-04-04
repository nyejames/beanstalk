can_error |name String| -> String, Float, Error!:
    return! Error("Other", "test.synthetic_boom", "boom")
;

recover |name String| -> String, Float:
    value, score = can_error(name) err! "guest", 0.0:
        io(err.message)
    ;
    return value, score
;

name, score = recover("Ana")
io(name)
io(score)
