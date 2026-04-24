-- Loop + match + result fallback combined inside a dispatcher path.

step_value |i Int| -> Int, Error!:
    if i is 2:
        return! Error("Step", "loop.match", "fail")
    ;
    return i * 10
;

i ~= 0
sum ~= 0

loop i < 4:
    value = step_value(i) ! 5

    if value is:
        case 0 => sum = sum + 1
        case 10 => sum = sum + 10
        case 20 if i > 0 => sum = sum + 20
        case 30 => sum = sum + 30
        else => sum = sum + 100
    ;

    i = i + 1
;

[:dispatcher_loop_match_result_fallback sum=[sum]]
