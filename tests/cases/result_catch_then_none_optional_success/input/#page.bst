find_name || -> String?, Error!:
    return! Error("boom")
;

recover || -> String?:
    return find_name() catch:
        then none
    ;
;

value = recover()
if value is none:
    io("then_none_ok")
;
