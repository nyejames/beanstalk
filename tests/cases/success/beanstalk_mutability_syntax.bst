-- Test file: Beanstalk mutability syntax with ~ operator
-- Tests that ~ syntax creates mutable access patterns

-- Mutable variable declarations (use ~= for initial assignment)
counter ~= 0
value ~= 10
message ~= "start"
flag ~= false

-- Mutable operations (use = for mutations after declaration)
counter = counter + 1
value = value * 2
message = "updated"
flag = not flag

-- Mutable borrowing (compiler determines reference vs ownership)
data = 42
mut_ref ~= data         -- Mutable access to data

-- Test compiler-determined ownership transfer
original_data = 100
moved_data ~= original_data  -- Compiler decides: move or mutable reference

-- Complex mutable operations
accumulator ~= 0
accumulator = accumulator + 5
accumulator = accumulator - 2
accumulator = accumulator * 3