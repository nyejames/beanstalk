pair_err |name String| -> String, Float, Error!:
    return! Error("Other", "test.synthetic_boom", "boom")
;

recover || -> String, Float:
    first, score = pair_err("Ana") ! "x", 1.0
    return first, score
;

name, score = recover()
io(name)
io(score)
