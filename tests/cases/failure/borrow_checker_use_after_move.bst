-- Test file: Use after move (should fail borrow checker)

-- Declare a mutable variable
data ~= ["important data"]

-- Move the data to another variable
moved_data = data

-- INVALID: Trying to use data after it was moved
result = [data: more text]