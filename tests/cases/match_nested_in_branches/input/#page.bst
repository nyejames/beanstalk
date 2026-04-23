outer ~= 1
result ~= "unset"

if outer is:
    case 1 =>
        inner ~= 2
        if inner is:
            case 2 => result = "one-two"
            else => result = "one-other"
        ;
    else => result = "other"
;

[:match_nested_in_branches result=[result]]
