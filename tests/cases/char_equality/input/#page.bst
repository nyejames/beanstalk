-- Test Char equality and inequality operators.
a = 'x'
b = 'x'
c = 'y'

-- Same chars are equal.
same = a is b
-- Different chars are not equal.
different = a is not c

io(same)
io(different)
