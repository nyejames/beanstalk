data = "loop_value"
found ~= false
counter ~= 0

loop counter < 5:
    if counter is 2:
        alias = data
        io(alias)
        found = true
        break
    ;
    counter = counter + 1
;

io(found)
