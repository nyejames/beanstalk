import @core/time {
    Duration,
    duration_from_seconds,
    duration_from_milliseconds,
}

-- Deterministic values, not live current time
d1 = duration_from_seconds(1.5)
d2 = duration_from_milliseconds(-250.0)
d3 = duration_from_milliseconds(500.0)

sec = d1.as_seconds()
millis = d1.as_milliseconds()
neg = d2.is_negative()
abs_d = d2.abs()
clamped = d3.clamp(duration_from_milliseconds(0.0), duration_from_milliseconds(1000.0))
clamped_sec = clamped.as_seconds()

io([:duration_conversions sec=[sec] millis=[millis] neg=[neg] abs_sec=[abs_d.as_seconds()] clamped_sec=[clamped_sec]])
