import @./impl { Label, make_label_impl }

PublicLabel as Label

make_label |text String| -> PublicLabel:
    return make_label_impl(text)
;
