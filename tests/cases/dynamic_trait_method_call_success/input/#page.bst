DISPLAY_TEXT must:
    display |This| -> String
;

RESETTABLE must:
    reset |~This|
;

Label = |
    text String,
|

display |this Label| -> String:
    return this.text
;

reset |this ~Label|:
    this.text = "reset"
;

Label must DISPLAY_TEXT
Label must RESETTABLE

render_dynamic |value DISPLAY_TEXT| -> String:
    return value.display()
;

reset_dynamic |value ~RESETTABLE|:
    ~value.reset()
;

[:dynamic_trait_method_call_success]
