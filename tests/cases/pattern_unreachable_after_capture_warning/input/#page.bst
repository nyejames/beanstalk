value ~= "hello"
result ~= "unset"

if value is:
    case captured => result = captured
    case "one" => result = "one"
    case "two" => result = "two"
    else => result = "other"
;

[:pattern_unreachable_after_capture_warning result=[result]]
