-- Test file: Last-use precision allows reborrow (should pass)
-- Tests requirement 7.3: last-use precision should allow reborrow after last use

-- Create a mutable variable
data ~= 10

-- Borrow the variable
data_ref = data

-- Use the reference (this should be the last use)
temp = data_ref

-- After last use, mutable borrow should be allowed
mut_data_ref ~= data

-- Use the mutable reference
mut_data_ref ~= 42