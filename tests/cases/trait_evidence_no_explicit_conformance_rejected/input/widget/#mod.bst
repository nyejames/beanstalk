import @./impl { Label, make_label_impl }

export @./impl { Label }

export PublicLabel as Label

export make_label || -> PublicLabel:
    return make_label_impl()
;
