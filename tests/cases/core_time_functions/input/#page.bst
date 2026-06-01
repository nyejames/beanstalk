import @core/time {
    Duration,
    TimeMark,
    Timestamp,
    mark_now,
    elapsed_since,
    duration_between,
    timestamp_now,
    duration_from_seconds,
    duration_from_milliseconds,
    timestamp_from_unix_seconds,
    timestamp_from_unix_milliseconds,
}

-- Duration construction and conversions
d1 = duration_from_seconds(1.5)
d2 = duration_from_milliseconds(-250.0)
sec = d1.as_seconds()
millis = d1.as_milliseconds()
neg = d2.is_negative()
abs_d = d2.abs()
clamped = d1.clamp(duration_from_milliseconds(0.0), duration_from_milliseconds(1000.0))
clamped_sec = clamped.as_seconds()

-- TimeMark delta (use deterministic derived values, not live elapsed)
mark1 = mark_now()
mark2 = mark_now()
delta = duration_between(mark1, mark2)
elapsed = elapsed_since(mark1)
delta_ok = delta.as_milliseconds() >= 0.0
elapsed_ok = elapsed.as_milliseconds() >= 0.0

-- Timestamp construction and conversions
ts_now = timestamp_now()
ts0 = timestamp_from_unix_milliseconds(0.0)
ts1 = timestamp_from_unix_seconds(1.0)
ts_millis = ts0.unix_milliseconds()
ts_sec = ts1.unix_seconds()
ts_iso = ts0.to_iso_string()

io([:duration sec=[sec] millis=[millis] neg=[neg] abs_sec=[abs_d.as_seconds()] clamped_sec=[clamped_sec]])
io([:delta_ok=[delta_ok] elapsed_ok=[elapsed_ok]])
io([:timestamp ts_millis=[ts_millis] ts_sec=[ts_sec] iso=[ts_iso]])
