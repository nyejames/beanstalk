-- Logger utility module
#log |message String|:
    io([message])
;

#debug |message String|:
    io([message])
;

-- Logger module initialization
io("Logger module initialized")
