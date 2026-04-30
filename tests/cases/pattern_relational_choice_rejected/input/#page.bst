-- Relational pattern on choice should fail

Status :: Ready, Loading;

value ~= Status::Ready
result ~= ""

if value is:
    case < Status::Loading => result = "bad"
    else => result = "fallback"
;

[:should_not_reach=[result]]
