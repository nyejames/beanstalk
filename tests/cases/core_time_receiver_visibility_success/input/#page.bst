import @core/time {Duration, duration_from_seconds, duration_from_milliseconds}

-- Receiver method visibility: importing the type should make receiver methods available
d = duration_from_seconds(2.5)
sec = d.as_seconds()
millis = d.as_milliseconds()
neg = d.is_negative()
abs_d = d.abs()
clamped = d.clamp(duration_from_milliseconds(0.0), duration_from_milliseconds(3000.0))
clamped_sec = clamped.as_seconds()

io([:receiver sec=[sec] millis=[millis] neg=[neg] abs_sec=[abs_d.as_seconds()] clamped_sec=[clamped_sec]])
