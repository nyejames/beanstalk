-- Test host functions with control flow structures
condition = true
counter ~= 0

print("Starting control flow test")

if condition:
    print("Condition is true")
    counter = counter + 1
    print("Counter incremented: " + counter)
else
    print("Condition is false")
;

-- Test with variables and expressions
x = 10
y = 5

if x > y:
    result = x + y
    print("x is greater: " + x + " + " + y + " = " + result)
else
    result = x - y
    print("x is not greater: " + x + " - " + y + " = " + result)
;

print("Control flow test complete")