CLONEABLE must:
    clone |This| -> This
;

Label = |
    text String,
|

clone |this Label| -> Label:
    return Label(this.text)
;

Label must CLONEABLE

keep type T is CLONEABLE |value T| -> T:
    return value
;

label = keep(Label("static"))

[:dynamic_trait_bound_only_static_bound_success [label.text]]
