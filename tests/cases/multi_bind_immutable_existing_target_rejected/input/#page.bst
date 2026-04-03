pair || -> String, Int:
    return "a", 1
;

existing = "old"
existing, created = pair()
io(existing)
io(created)
