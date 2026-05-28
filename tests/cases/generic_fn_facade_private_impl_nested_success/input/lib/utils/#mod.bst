import @./internal {wrap_impl}

wrap type T |value T| -> T:
    return wrap_impl(value)
;
