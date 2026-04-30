import @core/io/IO

#Response :: Success, Err | handle IO |;

a = Response::Success
b = Response::Success

if a is b:
    io("unreachable")
;
