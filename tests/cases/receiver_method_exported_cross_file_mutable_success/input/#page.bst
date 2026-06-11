import @helper { Resettable }

r ~= Resettable(42)
~r.reset()
io(r.value)
