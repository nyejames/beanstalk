-- Test only variable declarations (no mutations)

-- Basic variable declarations
x ~= 10
y ~= 20
z ~= 30

-- Arithmetic expressions (should fold at compile time)
sum ~= 5 + 3
product ~= 4 * 6
difference ~= 15 - 7