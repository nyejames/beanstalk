can_error |name String| -> String, Int, Error!:
    return! Error("Other", "test.synthetic_boom", "boom")
;

bubble |name String| -> String, Int, Error!:
    value, number = can_error(name) err!:
        return! err.bubble()
    ;
    return value, number
;

first, count = bubble("Ana") ! "fallback", 0
io(first)
io(count)
