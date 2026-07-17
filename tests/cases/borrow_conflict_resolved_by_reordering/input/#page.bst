-- Resolves a shared/mutable access conflict by using the shared alias before
-- creating the mutable alias. The shared alias is no longer live once the
-- mutable access begins, so this must compile (BST-BORROW-0003 is avoided).

data ~= 42
shared = data
io.line([: [shared]])
mutable_ref ~= data
mutable_ref = 7
io.line([: [mutable_ref]])
