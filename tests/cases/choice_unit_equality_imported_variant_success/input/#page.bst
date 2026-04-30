import @choices/Traffic

a Traffic = Traffic::Red
b Traffic = Traffic::Red
c Traffic = Traffic::Green

if a is b:
    io("same=true")
;

if a is c:
    io("different=true")
else
    io("different=false")
;
