-- Test file: Lifetime inference with precise error locations (should pass)
-- Tests that corrected lifetime information enables accurate error location reporting

-- Create data for borrowing
data = "shared data"

-- Sequential shared borrows (should work)
ref1 = data
ref2 = data
result1 = ref1
result2 = ref2

-- After shared references are done, mutable access should work
mut_ref ~= data