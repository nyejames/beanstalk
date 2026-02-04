-- Test variable declarations and assignments in JavaScript backend
-- Validates that the JS backend correctly handles let declarations
-- and reassignments with proper scoping

main ||:
    -- First assignment should use 'let'
    x = 10
    io("x = ", x)
    
    -- Second assignment should be reassignment
    x = 20
    io("x = ", x)
    
    -- Multiple variables
    a = 1
    b = 2
    c = 3
    io("a = ", a, ", b = ", b, ", c = ", c)
    
    -- String variables
    name = "Alice"
    greeting = "Hello"
    io(greeting, ", ", name)
    
    -- Boolean variables
    flag1 = true
    flag2 = false
    io("flag1 = ", flag1, ", flag2 = ", flag2)
;
