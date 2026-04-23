import @choices/Traffic
import @choices/identity

initial Traffic = Traffic::Red
next Traffic = identity(initial)

label ~= "unset"
if next is:
    case Red => label = "red";
    case Yellow => label = "yellow";
    case Green => label = "green";
;
[:choice_cross_file_runtime initial=[label] next=[label]]
