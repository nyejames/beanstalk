scores ~{String = Int} = {"Ada" = 10}

loop scores |entry|:
    io.line([: [entry]])
;

[:test]
