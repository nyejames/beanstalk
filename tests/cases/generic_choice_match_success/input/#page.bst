ResultShape type T, E ::
    Ok | value T |,
    Err | error E |,
;

get_value ResultShape of String, Error = ResultShape::Ok("hello")

if get_value is:
    case Ok(value) => io(value)
    case Err(error) => io(error.message)
;

[: generic choice match success]
