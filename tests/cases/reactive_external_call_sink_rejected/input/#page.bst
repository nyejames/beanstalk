-- Reactive template values passed to external/host calls are a deferred sink.

count $Int = 1

io([:reactive_external_call_sink_rejected [$(count)]])
