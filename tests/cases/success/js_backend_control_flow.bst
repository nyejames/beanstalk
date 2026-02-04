-- Test control flow structures in JavaScript backend
-- Validates if-else, loops, and pattern matching

main ||:
    -- Test if-else
    x = 10
    if x > 5:
        io("x is greater than 5")
    else
        io("x is not greater than 5")
    ;
    
    -- Test nested if
    y = 15
    if y > 10:
        if y < 20:
            io("y is between 10 and 20")
        ;
    ;
    
    -- Test pattern matching
    value = 42
    if value is:
        0: io("zero");
        42: io("forty-two");
        else: io("other");
    ;
    
    -- Test loop
    sum = 0
    loop i in 1 to 5:
        sum = sum + i
    ;
    io("Sum 1-5 = ", sum)
    
    -- Test loop with collection
    numbers = {1, 2, 3, 4, 5}
    total = 0
    loop num in numbers:
        total = total + num
    ;
    io("Total = ", total)
    
    io("Control flow tests passed")
;
