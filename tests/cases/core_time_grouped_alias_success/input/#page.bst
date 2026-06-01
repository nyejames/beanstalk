import @core/time {
    Duration,
    duration_from_seconds as seconds,
    duration_from_milliseconds as millis,
}

-- Aliased free functions
d1 = seconds(1.5)
d2 = millis(500.0)

sec = d1.as_seconds()
m = d2.as_milliseconds()

io([:alias sec=[sec] millis=[m]])
