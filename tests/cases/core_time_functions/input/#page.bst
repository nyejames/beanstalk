import @core/time {now_millis, now_seconds}

millis = now_millis()
seconds = now_seconds()

io([:core_time millis=[millis] seconds=[seconds]])
