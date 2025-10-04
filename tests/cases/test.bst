-- Simple return statement test
simple_function |x Int| -> Int:
    return x + 1
;

-- Test function call
result = simple_function(42)
print("Result: ")