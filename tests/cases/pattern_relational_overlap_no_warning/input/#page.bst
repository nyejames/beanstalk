-- Relational overlap should not produce warnings

value = 3
result ~= ""

if value is:
    case < 10 => result = "less_than_ten"
    case < 5 => result = "less_than_five"
    else => result = "other"
;

[:pattern_relational_overlap_no_warning result=[result]]
