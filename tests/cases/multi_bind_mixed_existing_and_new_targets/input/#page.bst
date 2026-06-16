pair || -> String, Int:
    return "fresh", 9
;

existing ~= "old"
existing, created = pair()
io.line([: [existing]])
io.line([: [created]])
