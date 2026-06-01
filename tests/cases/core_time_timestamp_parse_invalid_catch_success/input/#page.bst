import @core/time {
    Timestamp,
    timestamp_from_iso_string,
    timestamp_from_unix_milliseconds,
    to_iso_string,
}

-- Catch invalid ISO string and recover to a fallback
ts = timestamp_from_iso_string("not-a-date") catch:
    then timestamp_from_unix_milliseconds(0.0)
;

millis = ts.unix_milliseconds()
io([:timestamp_parse_catch millis=[millis]])
