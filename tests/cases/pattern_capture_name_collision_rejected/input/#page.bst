value ~= 5
existing ~= "outer"

if value is:
    case existing => io(existing)
    else => io("other")
;
