-- Simple if statement test for WASM control flow

-- Basic if statement
condition = true
if condition:
    result = 42
else:
    result = 0
;

-- Simple nested if
x = 10
if x > 5:
    if x > 8:
        nested_result = 1
    else:
        nested_result = 2
    ;
else:
    nested_result = 0
;