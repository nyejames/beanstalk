-- Error bubble through multiple layers, verifying trace frames at runtime.

inner || -> String, Int, Error!:
    return! Error("Test", "trace.multilayer", "origin")
;

middle || -> String, Int, Error!:
    v, n = inner()!
    return v, n
;

outer || -> String, Int, Error!:
    value, number = middle() err!:
        bubbled = err.bubble()
        return! bubbled
    ;
    return value, number
;

result, count = outer() ! "outer_handler", 0
io(result)
io(count)
