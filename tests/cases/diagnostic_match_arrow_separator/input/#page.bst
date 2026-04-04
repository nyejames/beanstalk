-- Match arms use '=>' and should reject '->'
value = 1
if value is:
    case 1 -> io("one");
;
