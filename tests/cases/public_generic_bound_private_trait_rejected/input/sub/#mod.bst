import @./internal { PRIVATE_DISPLAY }

identity type T is PRIVATE_DISPLAY |value T| -> T:
    return value
;
