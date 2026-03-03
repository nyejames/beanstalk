-- Test file: Disjoint variable borrows (should pass)

-- Create separate variables
x_value ~= 10
y_value ~= 20

-- Borrow different variables - this should be allowed
x_ref = x_value
y_ref = y_value

-- Use both references - should work fine
sum = x_ref + y_ref