-- Test file: Mutable and immutable borrow conflict (should fail borrow checker)

-- Declare a mutable variable
data ~= ["conflicted data"]

-- Create immutable reference
immutable_ref = data

-- INVALID: Try to create mutable reference while immutable reference exists
mutable_ref ~= data

-- Use both references (this should trigger borrow checker error)
-- Because if immutable_ref is not used after the mutable_ref, 
-- then its safe.
result = immutable_ref + mutable_ref