value ~= "hello"

if value is:
    captured => io(captured)
    else => io("fallback")
;

-- captured should not be visible outside the arm
io(captured)
