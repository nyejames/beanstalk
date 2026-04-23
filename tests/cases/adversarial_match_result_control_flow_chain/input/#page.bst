read_value |n Int| -> Int, Error!:
    if n is 2:
        return! Error("Read", "adversarial.match.read", "bad value")
    ;

    return n + 1
;

i ~= 0
sum ~= 0
label ~= "unset"

loop i < 5:
    current = read_value(i) ! -10

    if current is:
        case 1 => sum = sum + 100
        case 3 => sum = sum + 30
        case 5 if i > 2 => sum = sum + 5
        else => sum = sum + current
    ;

    if sum > 50:
        label = "high"
    else
        label = "low"
    ;

    i = i + 1
;

[:adversarial_match_result_control_flow_chain sum=[sum] label=[label]]
