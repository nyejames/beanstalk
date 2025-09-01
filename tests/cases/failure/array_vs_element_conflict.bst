-- Test file: Shared and mutable borrow conflict (should fail)
-- Tests that shared and mutable borrows should conflict

-- Create a mutable variable
data ~= "conflicted data"

-- Create immutable reference
immutable_ref = data

-- INVALID: Try to create mutable reference while immutable reference exists
mutable_ref ~= data

-- Use both references (this should trigger borrow checker error)
result = immutable_ref + mutable_ref