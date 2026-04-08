-- Test Char ordering operators (Unicode codepoint order).
a = 'a'
b = 'b'
z = 'z'

-- 'a' < 'b' is true.
a_lt_b = a < b
-- 'z' > 'a' is true.
z_gt_a = z > a
-- 'a' <= 'a' is true.
a_le_a = a <= a
-- 'b' >= 'a' is true.
b_ge_a = b >= a

io(a_lt_b)
io(z_gt_a)
io(a_le_a)
io(b_ge_a)
