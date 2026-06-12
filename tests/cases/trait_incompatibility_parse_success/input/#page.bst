A must:
    req |This| -> String
;

B must:
    req |This| -> String
;

Wrapper = |
    value String,
|

req |this Wrapper| -> String:
    return this.value
;

Wrapper must A

A must not B

io("ok")
