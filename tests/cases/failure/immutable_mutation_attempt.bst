-- Test file: Attempting to mutate immutable variables (should fail)

-- Declare an immutable variable
constant_value = 100

-- INVALID: Trying to mutate an immutable variable (should use ~= for mutable declaration)
constant_value = 200