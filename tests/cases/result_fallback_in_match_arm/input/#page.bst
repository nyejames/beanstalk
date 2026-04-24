-- Fallback path inside a match arm body.

get_value |n Int| -> String, Error!:
    if n is 2:
        return! Error("Test", "match.arm", "fail")
    ;
    return "ok"
;

selector ~= 2
value ~= ""

if selector is:
    case 1 =>
        value = get_value(1) ! "arm1_fallback"
    case 2 =>
        value = get_value(2) ! "arm2_fallback"
    else =>
        value = get_value(3) ! "else_fallback"
;

io(value)
