value ~= 7
result ~= "unset"

if value is:
    case captured if captured > 5 => result = "big"
    case captured => result = "small"
    else => result = "fallback"
;

[:pattern_capture_guard_success result=[result]]
