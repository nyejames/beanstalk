pair || -> String, Int:
    return "fresh", 9
;

existing ~= "old"
existing, created = pair()
io(existing)
io(created)
