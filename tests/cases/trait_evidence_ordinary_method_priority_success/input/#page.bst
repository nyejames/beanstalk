DISPLAY_A must:
    display |This| -> String
;

DISPLAY_B must:
    display |This| -> String
;

Label = |
    text String,
|

display |this Label| -> String:
    return this.text
;

Label must DISPLAY_A, DISPLAY_B

value = Label("ordinary wins")

[:trait_evidence_ordinary_method_priority_success [value.display()]]
