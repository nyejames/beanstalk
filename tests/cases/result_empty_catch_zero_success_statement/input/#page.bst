fail || -> Error!:
    return! Error("zero_success_empty_catch")
;

outer ||:
    fail() catch:
    ;
;

outer()
io("empty_catch_zero_ok")
