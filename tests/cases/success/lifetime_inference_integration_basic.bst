-- Test file: Basic lifetime inference integration (should pass)
-- Tests that the fixed lifetime inference integrates correctly with the full borrow checker

-- Simple variable operations with proper lifetime inference
data = "test data"
result = data

-- Mutable variable with proper last-use analysis
counter ~= 0
counter = counter + 1

-- Simple print to test basic functionality
io("Integration test completed")