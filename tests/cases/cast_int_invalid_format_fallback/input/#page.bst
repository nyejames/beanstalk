-- Prove that an invalid Int() cast triggers the fallback branch through final output.

parse_int_bad || -> String:
    raw = "not_a_number"
    value = Int(raw) ! 99
    return [:cast_int_fallback value=[value]]
;

[:[parse_int_bad()]]
