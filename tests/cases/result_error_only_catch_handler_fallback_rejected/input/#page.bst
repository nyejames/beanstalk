fail || -> Error!:
    return! Error("boom")
;

recover ||:
    fail() catch |err|:
        io(err.message)
        then "fallback"
    ;
;

recover()
