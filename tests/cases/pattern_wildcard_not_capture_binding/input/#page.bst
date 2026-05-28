Result :: Ok, Err | message String |;

result = Result::Err("bad")
if result is:
    Err(_) => io("err")
    else => io("other")
;
