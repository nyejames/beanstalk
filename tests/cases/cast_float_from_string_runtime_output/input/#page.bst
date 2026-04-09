-- Prove Float() cast from a valid string produces a correct value through final output.

parse_float || -> String:
    raw = "3.14"
    value = Float(raw) ! 0.0
    return [:cast_float_output value=[value]]
;

[:[parse_float()]]
