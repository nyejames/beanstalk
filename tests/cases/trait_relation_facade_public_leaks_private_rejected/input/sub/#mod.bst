export PUBLIC_TRAIT must:
    do_it |This| -> String
;

PRIVATE_TRAIT must:
    do_it |This| -> String
;

export PUBLIC_TRAIT must not PRIVATE_TRAIT
