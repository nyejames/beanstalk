ResultShape type T, E ::
    Ok | value T |,
    Err | error E |,
;

get_result |result ResultShape of Int, String| -> ResultShape of String, Int:
    return result
;

[: generic choice success]
