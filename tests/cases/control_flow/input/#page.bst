-- Test control flow structures in JavaScript backend
-- Validates if-else, loops, and pattern matching

-- Test if-else
x = 10
if x > 5:
    io("x is greater than 5")
else
    io("x is not greater than 5")
;

-- Test nested if
y = 15
if y > 10:
    if y < 20:
        io("y is between 10 and 20")
    ;
;

-- Test pattern matching
value = 42
if value is:
    case 0 => io("zero");
    case 42 => io("forty-two");
    else => io("other");
;

-- Test loop
sum ~= 0
loop 1 to 5 |i|:
    sum = sum + i
;
io([: Sum 1-5 = [sum]])

-- Secondary loop pass
total ~= 0
loop 1 to 5 |num|:
    total = total + num
;
io([: Total = [total]])

io("Control flow tests passed")
