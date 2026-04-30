value = 1
result ~= "unset"

if value is:
    case 1 => result = "one"
    case 1 => result = "one again"
    case captured => result = "captured"
    case 2 => result = "two"
    case 3 => result = "three"
    else => result = "other"
;

[:pattern_unreachable_combined_warning result=[result]]
