import @core/time { Duration, duration_from_seconds }

as_seconds |this Duration| -> String:
    return "shadowed"
;

duration = duration_from_seconds(3.5)
io(duration.as_seconds())
