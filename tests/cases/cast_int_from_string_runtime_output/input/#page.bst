-- Prove Int() cast from a valid string produces a correct value through final output.

parse_int || -> String:
    raw = "42"
    value = Int(raw) ! 0
    return [:cast_int_output value=[value]]
;

[:[parse_int()]]
