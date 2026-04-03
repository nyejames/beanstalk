-- Test that collection index access routes through __bs_index in the JS backend.
-- Validates the computed-place layer for collection element reads.

nums = {10, 20, 30}
io([: first: nums.get(0)])
io([: last: nums.get(2)])
