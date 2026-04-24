-- Relational patterns without default should fail

value ~= 5
result ~= ""

if value is:
    case < 0 => result = "negative"
    case >= 0 => result = "non-negative"
;

[:should_not_reach=[result]]
