import @./prefix/prefix

#compose |value String| -> String:
    left = prefix()
    return [:[left]-[value]-B]
;
