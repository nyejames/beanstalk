-- Wildcard pattern '_' is permanently rejected.

value ~= 1
if value is:
    _ => io("one")
    else => io("other")
;
