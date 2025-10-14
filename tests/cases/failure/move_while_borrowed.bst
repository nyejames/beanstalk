-- Test file: Move while borrowed (should fail)
-- Tests requirement 7.4: move-while-borrowed should be prevented

-- Create a variable
data ~= [:important data]

-- Borrow the variable
data_ref = data

-- INVALID: Try to move the variable while it's borrowed
moved_data = data

-- Use the reference (this should trigger borrow checker error)
result = data_ref + moved_data