import @core/time {duration_from_seconds}

-- Duration type is not imported, so receiver methods should not be visible.
d = duration_from_seconds(1.0)
bad = d.as_seconds()

io(bad)
