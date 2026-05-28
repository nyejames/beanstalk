import @./internal {keep}

exported_keep type T |value T| -> T?:
    return keep(value)
;
