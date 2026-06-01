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

value DISPLAY_TEXT = Label("wasm")

[:wasm dynamic=[value.display()]]
