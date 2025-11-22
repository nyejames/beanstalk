-- Test CoerceToString functionality with io() function
-- This tests compile-time constant folding for different types

-- Test integer literal (should fold to "42" at compile time)
io(42)

-- Test float literal (should fold to "3.14" at compile time)
io(3.14)

-- Test boolean literal (should fold to "true" at compile time)
io(true)

-- Test string literal (should pass through without conversion)
io("Hello, World!")

-- Test variable (will need runtime conversion when implemented)
x ~= 100
io(x)
