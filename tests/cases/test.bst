-- Complex control flow test for structured borrow checking

-- Test nested if statements with different variable usage patterns
x = 10
y = 20
temp ~= 0

if x > 5:
    if y > 15:
        result = x + y
        temp = result * 2
    else
        result = x - y
        temp = result / 2
    ;
    final_result = temp + 1
else
    if y < 25:
        result = y * 2
        temp = result + x
    else
        result = y / 2
        temp = result - x
    ;
    final_result = temp - 1
;

-- Test simple assignment after control flow
counter ~= 0
counter = counter + 1
value = counter * 2