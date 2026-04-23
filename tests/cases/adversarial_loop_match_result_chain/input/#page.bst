read_value |flag Bool| -> Int, Error!:
    if flag:
        return 10
    ;

    return! Error("Read", "read.failed", "no value")
;

i ~= 0
sum ~= 0

loop i < 4:
    value = read_value(i is not 2) ! 1

    if value is:
        case 10 => sum = sum + value
        else => sum = sum + 2
    ;

    i = i + 1
;

io(sum)
