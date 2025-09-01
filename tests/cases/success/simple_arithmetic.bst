-- Simple arithmetic test without problematic operators

-- Basic variable declarations
x ~= 10
y ~= 20
z ~= 30

-- Simple arithmetic (should fold at compile time)
sum ~= 5 + 3
product ~= 4 * 6
difference ~= 15 - 7

-- Variable mutation (proper syntax)
x = x + 1