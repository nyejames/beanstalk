-- Constant soft dependency: second references first even though first is defined later.
-- This tests that dependency sorting produces complete declaration placeholders.

#second Int = first + 2
#first Int = 40

[:[second]]
