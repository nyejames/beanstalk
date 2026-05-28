ErrorKind = |
    message String,
|

log_error |message ErrorKind|:
    io(message.message)
;

log_error(ErrorKind("something went wrong"))
