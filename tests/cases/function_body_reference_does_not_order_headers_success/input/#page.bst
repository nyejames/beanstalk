value = first()
io(value)

first || -> String:
    return second()
;

second || -> String:
    return "body references do not order headers"
;
