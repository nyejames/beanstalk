value ~= "hello"
result ~= "unset"

if value is:
    case "one" => result = "one"
    case captured => result = captured
    else => result = "fallback"
;

[:pattern_capture_after_literal_warning_or_reachable result=[result]]
