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

unused_dynamic || -> String:
    value DISPLAY_TEXT = Label("unused")
    return value.display()
;

[:reachable-only]
