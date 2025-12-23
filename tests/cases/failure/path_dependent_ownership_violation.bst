-- Test case: Same place with different ownership outcomes on different paths
-- This should be rejected by Beanstalk's global consistency requirement

-- This test demonstrates a violation of the global ownership consistency invariant
-- where the same place (x) would have different ownership outcomes depending on
-- which control flow path is taken.

x = "test_value"
condition = true

if condition:
    -- On this path, x would be moved (if this is its last use)
    result1 ~= x
    io(result1)
else
    -- On this path, x would be borrowed (if x is used later)
    result2 ~= x
    io(result2)
    -- Additional use of x that would prevent the move in the else branch
    io(x)
;

-- This should cause a borrow checker error because:
-- 1. In the 'if' branch, x could be moved (if it's the last use)
-- 2. In the 'else' branch, x must be borrowed (because it's used again after result2)
-- 3. This violates Beanstalk's global consistency requirement