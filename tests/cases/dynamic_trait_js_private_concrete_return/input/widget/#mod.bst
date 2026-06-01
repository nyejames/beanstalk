import @./impl { make_secret }

make_public || -> DISPLAYABLE:
    return make_secret()
;
