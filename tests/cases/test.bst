-- Test file for development and debugging (should succeed)

-- Simple mutable variable declaration
test_var ~= 42

-- Mutable string assignment  
name ~= "Beanstalk"

-- Multiple mutable assignments
counter ~= 0
flag ~= true
value ~= 3.14

-- Immutable assignment for comparison
constant_value = 100

-- PROPER mutation of mutable variable (using = not ~=)
test_var = 84