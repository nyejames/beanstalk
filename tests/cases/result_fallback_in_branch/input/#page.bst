-- Fallback paths inside if-branch and else-branch bodies.

get_value |i Int| -> String, Error!:
    if i is 2:
        return! Error("Test", "branch", "fail")
    ;
    return "ok"
;

i ~= 0
loop i < 4:
    value ~= ""
    if i is 2:
        value = get_value(i) ! "branch_fallback"
    else
        value = get_value(i) ! "else_fallback"
    ;
    io(value)
    i = i + 1
;
