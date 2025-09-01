-- Test file: Mutable and immutable borrow conflict (should fail borrow checker)

-- Declare a mutable variable
data ~= "conflicted data"

-- Create immutable reference
immutable_ref = data

-- INVALID: Try to create mutable reference while immutable reference exists
mutable_ref ~= data

-- Use both references (this should trigger borrow checker error)
result = immutable_ref + mutable_ref