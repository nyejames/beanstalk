INT_LABEL must:
    label |This| -> String
;

label |this Int| -> String:
    return [:int=[this]]
;

Int must INT_LABEL

value = 7
[:trait_conformance_builtin_extension_success [value.label()]]
