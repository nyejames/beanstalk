-- Prove that an invalid Float() cast triggers the fallback branch through final output.

parse_float_bad || -> String:
    raw = "not_a_float"
    value = Float(raw) ! 9.9
    return [:cast_float_fallback value=[value]]
;

[:[parse_float_bad()]]
