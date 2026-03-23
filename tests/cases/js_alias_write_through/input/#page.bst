-- Test that implicit borrow creates an alias and write-through works.
-- Validates that __bs_assign_borrow and __bs_read are used for alias semantics.

x ~= 10
ref = x
io(ref)
