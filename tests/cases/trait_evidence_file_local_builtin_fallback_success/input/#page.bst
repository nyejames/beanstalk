INT_LABEL must:
    label |This| -> String
;

label |this Int| -> String:
    return "int evidence"
;

Int must INT_LABEL

value = 7

[:trait_evidence_file_local_builtin_fallback_success [value.label()]]
