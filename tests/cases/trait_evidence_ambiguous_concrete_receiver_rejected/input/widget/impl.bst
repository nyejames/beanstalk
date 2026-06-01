import @./traits { DISPLAY_A, DISPLAY_B }

Label = |
    text String,
|

make_label_impl || -> Label:
    return Label("ambiguous")
;

display |this Label| -> String:
    return this.text
;

Label must DISPLAY_A, DISPLAY_B
