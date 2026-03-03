-- Test recursive block emission with control flow

add |x Int, y Int| -> Int:
    return x + y
;

main ||:
    result = add(5, 3)
    io(result)
    
    -- Test if-else
    if result > 5:
        io("Greater than 5")
    else
        io("Not greater than 5")
    ;
    
    -- Test loop
    loop i in 0 to 3:
        io(i)
    ;
;
