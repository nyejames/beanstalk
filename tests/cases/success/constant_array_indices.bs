-- Test file: Different variable borrows (should pass)
-- Tests that borrowing different variables should be allowed

-- Create separate variables
first_value ~= 10
second_value ~= 20

-- Borrow different variables
first_ref = first_value
second_ref = second_value

-- Use both references - should work fine since they're different variables
sum = first_ref + second_ref