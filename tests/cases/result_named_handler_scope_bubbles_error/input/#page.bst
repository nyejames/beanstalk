can_error |name String| -> String, Int, Error!:
    return! Error("boom")
;

bubble |name String| -> String, Int, Error!:
    value, number = can_error(name) err!:
        return! err
    ;
    return value, number
;

first, count = bubble("Ana") ! "fallback", 0
io(first)
io(count)
