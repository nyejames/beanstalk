-- Test function definitions and calls in JavaScript backend
-- Validates that the JS backend correctly handles function
-- declarations, parameters, return values, and recursion

add |x Int, y Int| -> Int:
    return x + y
;

multiply |x Int, y Int| -> Int:
    return x * y
;

greet |name String| -> String:
    return "Hello, " + name
;

factorial |n Int| -> Int:
    if n <= 1:
        return 1
    ;
    return n * factorial(n - 1)
;

main ||:
    -- Test simple function calls
    sum = add(5, 3)
    io([: 5 + 3 = sum])
    
    product = multiply(4, 6)
    io([: 4 * 6 = product])
    
    -- Test string function
    message = greet("World")
    io(message)
    
    -- Test recursive function
    fact5 = factorial(5)
    io([: 5! = fact5])
    
    io("Function tests passed")
;
