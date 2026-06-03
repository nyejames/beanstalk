export Label = |
    text String,
|

export display |this Label| -> String:
    return this.text
;

Label must DISPLAYABLE
