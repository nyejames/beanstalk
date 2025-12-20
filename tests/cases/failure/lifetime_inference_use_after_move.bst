-- Test file: Lifetime inference use after move detection (should fail)
-- Tests that the fixed lifetime inference correctly detects use after move

-- Create data
original = "move me"

-- Move the data
moved ~= original

-- INVALID: Try to use original after it has been moved
invalid_use = original