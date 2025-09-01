-- Test file: Multiple mutable borrow conflict (should fail)
-- Tests requirement 7.2: multiple mutable borrows should be caught

-- Create a mutable variable
data ~= "shared data"

-- Create first mutable reference
ref1 ~= data

-- INVALID: Create second mutable reference while first is still active
ref2 ~= data

-- Use both references (this should trigger borrow checker error)
result = ref1 + ref2