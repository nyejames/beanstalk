-- Test file: Complex borrow violations (should fail)
-- Tests that complex borrowing patterns are properly rejected

-- Test nested borrowing with mutable conflict
data = "test data"
ref1 = data
ref2 = ref1

-- This should fail: trying to create mutable access while shared references exist
mut_ref ~= data  -- ERROR: shared/mutable conflict