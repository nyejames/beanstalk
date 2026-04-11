inner |name String| -> String, Int, Error!:
    if name is "":
        return! Error("Parse", "test.inner.empty", "empty")
    ;

    return name, 1
;

outer |name String| -> String, Int, Error!:
    value, number = inner(name) err!:
        return! err.bubble()
    ;

    return value, number
;

ok_name, ok_number = outer("Ana") ! "ok-fallback", 0
missing_name, missing_number = outer("") ! "missing-fallback", -1
io(ok_name)
io(ok_number)
io(missing_name)
io(missing_number)
