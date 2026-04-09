simple_function |value Int| -> Int:
    result = value + 1
    return result
;

number = 42
result = simple_function(number)
[:function_single_call_smoke result=[result]]
