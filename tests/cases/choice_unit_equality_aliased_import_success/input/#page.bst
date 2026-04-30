import @choices/Traffic/Traffic as Signal

a Signal = Signal::Red
b Signal = Signal::Red
c Signal = Signal::Green

if a is b:
    io("same=true")
;

if a is c:
    io("different=true")
else
    io("different=false")
;
