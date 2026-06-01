DISPLAY_TEXT must:
    display |This| -> String
;

Label = |
    text String,
|

display |this Label| -> String:
    return this.text
;

only_label |this Label| -> String:
    return "concrete"
;

Label must DISPLAY_TEXT

render_dynamic |value DISPLAY_TEXT| -> String:
    return value.only_label()
;

[:unreachable]
