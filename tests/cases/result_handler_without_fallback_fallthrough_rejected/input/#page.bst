can_error |value String| -> String, Int, Error!:
    return value, 1
;

recover |value String| -> String, Int:
    a, b = can_error(value) err!:
        io(err.message)
    ;
    return a, b
;

first, count = recover("Ana")
io(first)
io(count)
