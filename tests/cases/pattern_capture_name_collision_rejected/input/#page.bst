value ~= 5
existing ~= "outer"

if value is:
    existing => io(existing)
    else => io("other")
;
