-- Test file: Last-use precision allows reborrow (should pass)

-- Create a mutable variable
data ~= 10

-- Borrow the variable
data_ref = data

-- Use the reference (this should be the last use)
temp = data_ref

-- After last use, mutable borrow should be allowed
mut_data_ref ~= data

-- Use the mutable reference
mut_data_ref = 42