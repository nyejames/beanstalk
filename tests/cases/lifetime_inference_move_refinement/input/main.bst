-- Test file: Lifetime inference integration with move refinement (should pass)
-- Tests that corrected lifetime inference provides accurate last-use information for move refinement

-- Create data that will be moved
original_data = "move me"

-- This should become a move since original_data is not used after this point
moved_data ~= original_data

-- Use the moved data
result = moved_data