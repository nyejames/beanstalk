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

Holder = |
    value DISPLAY_TEXT,
|

Message ::
    Text | value DISPLAY_TEXT |,
;

argument_boundary |value DISPLAY_TEXT| -> String:
    return "argument"
;

declaration_boundary ||:
    value DISPLAY_TEXT = Label("declaration")
;

argument_call_boundary || -> String:
    return argument_boundary(Label("argument"))
;

return_boundary || -> DISPLAY_TEXT:
    return Label("return")
;

field_boundary || -> Holder:
    return Holder(Label("field"))
;

payload_boundary || -> Message:
    return Message::Text(Label("payload"))
;

collection_boundary || -> {DISPLAY_TEXT}:
    return {Label("first"), Label("second")}
;

block_declaration_boundary |condition Bool|:
    value DISPLAY_TEXT = if condition:
        then Label("then")
    else
        then Label("else")
    ;
;

[:dynamic_trait_coercion_boundaries_success]
