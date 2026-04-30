import @choices/Response

a Response = Response::Err("bad")
b Response = Response::Err("bad")
c Response = Response::Err("good")

if a is b:
    io("equal")
;

if a is c:
    io("different")
else
    io("not equal")
;
