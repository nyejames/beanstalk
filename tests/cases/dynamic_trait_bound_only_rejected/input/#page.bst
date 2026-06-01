CLONEABLE must:
    clone |This| -> This
;

accept_dynamic |value CLONEABLE| -> String:
    return "never"
;
