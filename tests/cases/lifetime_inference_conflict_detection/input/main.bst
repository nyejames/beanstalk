-- Test file: Lifetime inference conflict detection (should fail)
-- Tests that the fixed lifetime inference correctly detects borrow conflicts

-- Create data
data = "test data"

-- Create shared reference
shared_ref = data

-- INVALID: Try to create mutable reference while shared reference is still active
mut_ref ~= data

-- Use both references (keeps them live)
result1 = shared_ref
result2 = mut_ref