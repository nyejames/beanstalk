-- Dynamic if statement test for WASM control flow

-- Variable condition (not constant)
x ~= 10
y ~= 5

-- Dynamic if statement
result ~= -1
if x > y:
    result = 1
else
    result = 0
;

-- Another dynamic condition
-- This time there is a new declaration in each branch
if y > 3:
    another_result = 42
else
    another_result = 24
;