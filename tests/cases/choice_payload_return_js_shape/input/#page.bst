#Response :: Err | message String |, Success;

make_err |msg String| -> Response:
    return Response::Err(msg)
;

result = make_err("bad")
if result is:
    case Success => io("ok")
    case Err(message) => io("err")
;
