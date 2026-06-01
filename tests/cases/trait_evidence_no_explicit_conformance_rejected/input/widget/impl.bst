Label = |
    text String,
|

make_label_impl || -> Label:
    return Label("shape only")
;

display |this Label| -> String:
    return this.text
;
