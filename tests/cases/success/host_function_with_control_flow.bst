-- Test host functions with control flow structures
condition = true
counter ~= 0

["Starting control flow test"]

if condition:
    ["Condition is true"]
    counter = counter + 1
    ["Counter incremented: ", counter]
else
    ["Condition is false"]
;

-- Test with variables and expressions
x = 10
y = 5

if x > y:
    result = x + y
    ["x is greater: ", x, " + ", y, " = ", result])
else
    result = x - y
    ["x is not greater: ", x, " - ", y, " = ", result])
;

["Control flow test complete"]