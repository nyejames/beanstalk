-- Test host functions with control flow structures
condition = true
counter ~= 0

io("Starting control flow test")

if condition:
    io("Condition is true")
    counter = counter + 1
    io([: Counter incremented: counter])
else
    io("Condition is false")
;

-- Test with variables and expressions
x = 10
y = 5

if x > y:
    result = x + y
    io([: x is greater: x + y = result])
else
    result = x - y
    io([: x is not greater: x - y = result])
;

io("Control flow test complete")