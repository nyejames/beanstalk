-- Logger utility module
export log |message String| -> Void:
    io([: [LOG] message])
;

export debug |message String| -> Void:
    io([: [DEBUG] message])
;

-- Logger module initialization
io("Logger module initialized")