fail || -> Error!:
    return! Error("zero statement boom")
;

outer || -> Error!:
    fail()!
;

outer() catch |err|:
    io(err.message)
;
