-- Test user-defined receiver method on Char.
-- This verifies that the Char receiver infrastructure routes correctly.

is_letter_a |this Char| -> Bool:
    return this is 'a'
;

c = 'a'
d = 'b'

io(c.is_letter_a())
io(d.is_letter_a())
