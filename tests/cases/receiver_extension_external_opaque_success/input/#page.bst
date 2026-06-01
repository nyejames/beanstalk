import @core/time { Duration, duration_from_seconds }

seconds_label |this Duration| -> String:
    return [: seconds=[this.as_seconds()]]
;

duration = duration_from_seconds(3.5)
io([:receiver_extension_external_opaque_success [duration.seconds_label()]])
