a = true
b = false
c = false

grouped = (a or b) and c
ungrouped = a or b and c

if grouped:
    io("grouped")
;

if ungrouped:
    io("ungrouped")
;
