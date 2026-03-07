#wrapper = [: before [$slot("title")] after]
output = [wrapper:[$insert("missing"): nope]]
io(output)
