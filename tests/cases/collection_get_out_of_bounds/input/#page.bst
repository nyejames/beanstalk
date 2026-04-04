-- Tests collection get() with an out-of-bounds index handled via fallback.

values = {10, 20, 30}
result = values.get(99) ! -1
io(result)
