DISPLAY_TEXT must:
    display |This| -> String
;

Label = |
    text String,
|

display |this Label| -> String:
    return this.text
;

accept_dynamic |value DISPLAY_TEXT| -> String:
    return "unused"
;

result = accept_dynamic(Label("missing evidence"))
[:unreachable]
