data = "shared"
counter ~= 0

loop counter < 3:
    if counter is 1:
        alias = data
        io.line([: [alias]])
    else
        copied = copy(data)
        io.line([: [copied]])
    ;

    counter = counter + 1
;

final = data
io.line([: [final]])
