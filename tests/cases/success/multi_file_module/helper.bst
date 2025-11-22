-- Helper file for multi-file module test
export greet |name String| -> Void:
    io([: Hello, name!])
;

-- This is helper's implicit main (callable but doesn't auto-execute)
io("Helper module loaded")