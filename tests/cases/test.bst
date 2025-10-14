-- Simple function definition and call test
simple_function |value Int| -> Int:
    result = value + 1
    return result
;

number = 42
result = simple_function(number)