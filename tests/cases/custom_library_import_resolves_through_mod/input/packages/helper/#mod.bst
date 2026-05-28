import @./impl { add as internal_add }

add |a Int, b Int| -> Int:
    return internal_add(a, b)
;
