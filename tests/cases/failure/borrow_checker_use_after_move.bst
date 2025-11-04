-- Test file: Use after move (should fail borrow checker)

-- Declare a mutable variable
data ~= 42

-- Move the data to another variable with mutable access
-- This should be a move since data is not used after this
moved_data ~= data

-- INVALID: Trying to use data after it was moved
result = data