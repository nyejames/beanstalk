-- Test Char as a deliberate core datatype.
-- Inferred type from a char literal.
a ~= 'a'

-- Explicit Char type annotation.
b Char = 'b'

-- Mutable char with explicit annotation.
c ~Char = 'c'

io(a)
io(b)
io(c)
