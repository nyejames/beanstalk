can_error || -> String, Error!:
    return! Error("Other", "test.ifmatch", "boom")
;

recover || -> String:
    value = can_error() err! "fallback":
        if 1 is:
            case 1 => io("one");
            else => io("other");
        ;
    ;

    return value
;

output = recover()
io(output)
