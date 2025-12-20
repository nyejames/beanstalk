-- Test that should trigger Polonius-style conflict detection

-- Simple test with conflicting borrows
x ~= 42
y = x       -- Shared borrow of x
z ~= x      -- Mutable borrow of x - should conflict with shared borrow y
io(z)