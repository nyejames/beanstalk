Status :: Ready, Busy;

a Status = Status::Ready
b Status = Status::Ready
c Status = Status::Busy

if a is b:
    io("same=true")
;

if a is c:
    io("different=true")
else
    io("different=false")
;
