-- Non-choice wildcard match

value ~= 42
result ~= ""

if value is:
    case _ => result = "anything"
;

[:wildcard_result=[result]]
