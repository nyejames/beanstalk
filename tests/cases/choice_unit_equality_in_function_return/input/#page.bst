#Status :: Ready, Busy;

check_equal |a Status, b Status| -> Bool:
    return a is b
;

a Status = Status::Ready
b Status = Status::Ready
c Status = Status::Busy

if check_equal(a, b):
    io("same=true")
;

if check_equal(a, c):
    io("different=true")
else
    io("different=false")
;
