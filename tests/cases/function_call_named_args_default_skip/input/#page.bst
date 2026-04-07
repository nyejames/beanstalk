describe |prefix String = "item", subject String| -> String:
    return subject
;

-- Skip the defaulted 'prefix', provide only 'subject' by name
io(describe(subject = "apple"))
