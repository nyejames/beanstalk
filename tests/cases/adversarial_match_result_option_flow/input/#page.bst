pair_opt || -> String?, Int?:
    return none, none
;

step_value |idx Int| -> Int, Error!:
    if idx is 2:
        return! Error("Step", "adversarial.match.option", "boom")
    ;

    return idx + 1
;

i ~= 0
sum ~= 0

loop i < 4:
    name String?, count Int? = pair_opt()

    value = step_value(i) ! 0
    if value is:
        case 0 => sum = sum + 50
        case 2 if i < 2 => sum = sum + 20
        case 4 => sum = sum + 4
        else => sum = sum + 1
    ;

    if i is 3:
        i = i + 1
    else
        i = i + 1
    ;
;

[:adversarial_match_result_option_flow sum=[sum]]
