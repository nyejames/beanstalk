value = 1
result ~= "unset"

if value is:
    case 1 => result = "one"
    case 2 => result = "two"
    case 1 => result = "one again"
    else => result = "other"
;

[:pattern_duplicate_literal_warning result=[result]]
