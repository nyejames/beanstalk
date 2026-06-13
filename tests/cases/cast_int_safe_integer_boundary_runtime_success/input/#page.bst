parse_string |text String| -> Int, Error!:
    return cast! text
;

parse_float |value Float| -> Int, Error!:
    return cast! value
;

max_from_string = parse_string("9007199254740991") catch:
    then -1
;

min_from_string = parse_string("-9007199254740991") catch:
    then 1
;

max_from_float = parse_float(9007199254740991.0) catch:
    then -1
;

min_from_float = parse_float(-9007199254740991.0) catch:
    then 1
;

[: runtime safe int boundaries: [max_from_string]/[min_from_string]/[max_from_float]/[min_from_float]]
