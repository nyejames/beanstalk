import @core/time

-- Namespace value access for free functions
d = time.duration_from_seconds(1.0)
sec = d.as_seconds()

-- Namespace type-position access if supported
value time.Duration = time.duration_from_seconds(2.0)
millis = value.as_milliseconds()

io([:namespace sec=[sec] millis=[millis]])
