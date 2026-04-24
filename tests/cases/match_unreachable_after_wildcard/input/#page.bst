-- Unreachable arm after wildcard should fail

value ~= 1
result ~= ""

if value is:
    case _ => result = "anything"
    case 1 => result = "dead"
;

[:should_not_reach=[result]]
