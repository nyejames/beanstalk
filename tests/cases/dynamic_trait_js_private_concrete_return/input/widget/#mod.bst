import @./impl { make_secret }

export make_public || -> DISPLAYABLE:
    return make_secret()
;
