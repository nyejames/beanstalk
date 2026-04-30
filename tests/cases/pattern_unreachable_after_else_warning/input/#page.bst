value = 1
result ~= "unset"

if value is:
    case 1 => result = "one"
    else => result = "other"
    case 2 => result = "two"
    case 3 => result = "three"
;

[:pattern_unreachable_after_else_warning result=[result]]
