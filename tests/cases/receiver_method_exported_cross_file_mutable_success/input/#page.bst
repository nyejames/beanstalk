import @helper/Resettable
import @helper/reset

r ~= Resettable(42)
~r.reset()
io(r.value)
