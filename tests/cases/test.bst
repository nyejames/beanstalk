-- Test file for development and debugging (should succeed)
-- Simple mutable variable declaration
test_var ~= "test"

-- Move of test var into new variable (OR Mutable reference if its not used again later on)
moving_test_var ~= test_var

-- This will error if moving_test_var is used again later, but fine if not
-- Compiler should statically know whether the previous use was a move or mutable reference for one statement
test_var = "hello"

-- This will cause the previous mutation to be an error
-- You can comment this out to test this behaviour
moving_test_var = "world"