-- Test comparison and logical operations in JavaScript backend
-- Validates all comparison operators and boolean logic

main ||:
    -- Test equality
    a = 5
    b = 5
    c = 10
    
    if a is b:
        io("5 == 5: true")
    ;
    
    if a is not c:
        io("5 != 10: true")
    ;
    
    -- Test comparison operators
    if a < c:
        io("5 < 10: true")
    ;
    
    if c > a:
        io("10 > 5: true")
    ;
    
    if a <= b:
        io("5 <= 5: true")
    ;
    
    if c >= a:
        io("10 >= 5: true")
    ;
    
    -- Test logical AND
    x = true
    y = false
    
    if x and x:
        io("true AND true: true")
    ;
    
    if not (x and y):
        io("true AND false: false")
    ;
    
    -- Test logical OR
    if x or y:
        io("true OR false: true")
    ;
    
    if not (y or y):
        io("false OR false: false")
    ;
    
    -- Test logical NOT
    if not y:
        io("NOT false: true")
    ;
    
    -- Test complex expressions
    result = (a < c) and (c > a)
    if result:
        io("Complex expression: true")
    ;
    
    io("Comparison and logical tests passed")
;
