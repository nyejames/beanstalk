-- Test case: Inconsistent move outcomes for the same place
-- This should be rejected by Beanstalk's global consistency requirement

-- This test demonstrates another violation where the same place has
-- inconsistent ownership outcomes across different usage points

data = "shared_data"
some_condition = true

-- First usage: This could be a move if it's the last use
first_ref ~= data

if some_condition:
    -- Use first_ref, making it potentially the last use of data
    io(first_ref)
else
    -- Use first_ref differently
    io(first_ref)
    -- But then use data again, which would prevent the move
    second_ref ~= data
    io(second_ref)
;

-- This creates an inconsistency:
-- - If we take the 'if' branch, data could be moved to first_ref
-- - If we take the 'else' branch, data must be borrowed to first_ref (because it's used again)
-- This violates the global consistency requirement