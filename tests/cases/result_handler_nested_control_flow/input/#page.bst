inner |name String| -> String, Int, Error!:
    if name is "":
        return! Error("empty")
    ;

    return name, 1
;

outer |name String| -> String, Int, Error!:
    value, number = inner(name) catch |err|:
        return! err
    ;

    return value, number
;

ok_name, ok_number = outer("Ana") catch:

    then "ok-fallback", 0

;
missing_name, missing_number = outer("") catch:
    then "missing-fallback", -1
;
io(ok_name)
io(ok_number)
io(missing_name)
io(missing_number)
