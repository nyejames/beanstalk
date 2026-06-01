import @core/time {
    Timestamp,
    timestamp_from_unix_seconds,
    timestamp_from_unix_milliseconds,
    to_iso_string,
    unix_seconds,
    unix_milliseconds,
}

-- Use epoch values for stable output
ts0 = timestamp_from_unix_milliseconds(0.0)
ts1 = timestamp_from_unix_milliseconds(1000.0)
ts2 = timestamp_from_unix_seconds(1.0)

iso0 = ts0.to_iso_string()
iso1 = ts1.to_iso_string()
millis0 = ts0.unix_milliseconds()
sec1 = ts1.unix_seconds()
millis2 = ts2.unix_milliseconds()

io([:timestamp_conversions iso0=[iso0] iso1=[iso1] millis0=[millis0] sec1=[sec1] millis2=[millis2]])
