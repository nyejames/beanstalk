-- Logger utility module
#log |message String| -> Void:
    io([: [LOG] message])
;

#debug |message String| -> Void:
    io([: [DEBUG] message])
;

-- Logger module initialization
io("Logger module initialized")