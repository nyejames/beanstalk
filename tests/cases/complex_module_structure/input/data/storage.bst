-- Storage utility module
#save |key String, value Int|:
    io([: Saving [key] = [value]])
;

#load |key String| -> Int:
    io([: Loading [key]])
    return 42
;
