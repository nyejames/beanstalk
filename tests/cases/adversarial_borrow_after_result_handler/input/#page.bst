load_value |ok Bool| -> String, Error!:
    if ok:
        return "value"
    ;

    return! Error("Test", "test.load_failed", "failed")
;

value = load_value(true) ! "fallback"
alias1 = value
alias2 = alias1
io(alias2)
