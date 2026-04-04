-- Wildcard arm syntax should guide users to else:
value = 1
if value is:
    case _ => io("one");
;
