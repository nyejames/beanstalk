DISPLAY_TEXT must:
    display |This| -> String
;

display |this String| -> String:
    return this
;

String must DISPLAY_TEXT

make_label || -> DISPLAY_TEXT:
    return "extension"
;

value = make_label()

[:file-local=[value.display()]]
