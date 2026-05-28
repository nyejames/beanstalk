import @./helpers {identity}

run |value String|:
    io(value)
;

run(identity)
