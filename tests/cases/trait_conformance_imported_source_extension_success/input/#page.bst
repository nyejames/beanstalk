import @./models { Label }

LABEL_TEXT must:
    text |This| -> String
;

text |this Label| -> String:
    return this.value
;

Label must LABEL_TEXT

label = Label("imported")
[:trait_conformance_imported_source_extension_success [label.text()]]
