Secret = |
    text String,
|

display |this Secret| -> String:
    return this.text
;

Secret must DISPLAYABLE

make_secret || -> DISPLAYABLE:
    return Secret("private")
;
