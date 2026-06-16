Status :: Ready, Busy;

a Status = Status::Ready
b Status = Status::Ready
c Status = Status::Busy

if a is b:
    io.line("same=true")
;

if a is c:
    io.line("different=true")
else
    io.line("different=false")
;
