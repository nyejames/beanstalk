-- Relational int match patterns

value ~= 5
result ~= ""

if value is:
    case < 0 => result = "negative"
    case 0 => result = "zero"
    case <= 10 => result = "small"
    case _ => result = "large"
;

[:relational_result=[result]]
