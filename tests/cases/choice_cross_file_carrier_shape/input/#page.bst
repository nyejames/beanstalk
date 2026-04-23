import @choices/Traffic
import @choices/identity

initial Traffic = Traffic::Red
next Traffic = identity(initial)

result ~= "unset"
if next is:
    case Red => result = "red"
    case Yellow => result = "yellow"
    case Green => result = "green"
;
[:choice_cross_file_carrier_shape result=[result]]
