-- 'else =>' must not appear before any 'case' arm.

value ~= 1
result ~= ""

if value is:
    else => result = "first"
    case 1 => result = "second"
;

[:should_not_reach=[result]]
