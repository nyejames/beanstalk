-- Logger utility module
#log |message String|:
    io([message])
;

#debug |message String|:
    io([message])
;

#init_logger ||:
    io("Logger module initialized")
;
