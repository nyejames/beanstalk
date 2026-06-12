import @./impl { Label, make_label_impl }

export @./impl { Label }

export PublicLabel as Label

export make_label |text String| -> PublicLabel:
    return make_label_impl(text)
;
