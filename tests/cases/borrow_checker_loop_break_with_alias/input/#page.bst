data = "loop_value"
found ~= false
counter ~= 0

loop counter < 5:
    if counter is 2:
        alias = data
        io.line([: [alias]])
        found = true
        break
    ;
    counter = counter + 1
;

io.line([: [found]])
