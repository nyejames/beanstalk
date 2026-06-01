import @core/time {
    Timestamp,
    timestamp_from_iso_string,
    timestamp_from_unix_milliseconds,
    to_iso_string,
    unix_milliseconds,
}

parse_epoch_plus_one || -> Timestamp, Error!:
    return timestamp_from_iso_string("1970-01-01T00:00:01.000Z")!
;

-- Parse a valid ISO string with postfix propagation inside a fallible helper,
-- then recover at the top-level start boundary.
ts = parse_epoch_plus_one() catch:
    then timestamp_from_unix_milliseconds(0.0)
;

millis = ts.unix_milliseconds()
iso = ts.to_iso_string()

io([:timestamp_parse millis=[millis] iso=[iso]])
