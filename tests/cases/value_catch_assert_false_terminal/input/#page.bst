can_error |ok Bool| -> String, Error!:
    if ok:
        return "ok"
    ;

    return! Error("boom")
;

value = can_error(true) catch |err|:
    io(err.message)
    assert(false, "unreachable error path")
;

io("value catch assert false terminal")
io(value)
