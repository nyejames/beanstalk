#Response :: Ok | value Int |, Err | message String |;

a = Response::Ok(42)
b = Response::Err("bad")

if a is b:
    io("same")
else
    io("different")
;
