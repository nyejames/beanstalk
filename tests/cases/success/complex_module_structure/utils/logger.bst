-- Logger utility module
#log |message String| -> Void:
    io([message])
;

#debug |message String| -> Void:
    io([message])
;

-- Logger module initialization
io("Logger module initialized")