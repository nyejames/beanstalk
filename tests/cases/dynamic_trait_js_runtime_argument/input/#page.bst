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

render_label |value DISPLAY_TEXT| -> String:
    return value.display()
;

[:argument=[render_label(Label("argument"))]]
