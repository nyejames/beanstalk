DISPLAY_TEXT must:
    display |This| -> String
;

Label = |
    text String,
|

display |this Label| -> String:
    return this.text
;

Label must DISPLAY_TEXT

make_label || -> DISPLAY_TEXT:
    return Label("return")
;

value = make_label()

[:return=[value.display()]]
