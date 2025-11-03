-- Helper file for multi-file module test
export greet |name String| -> Void:
    ["Hello, " + name + "!"]
;

-- This is helper's implicit main (callable but doesn't auto-execute)
["Helper module loaded"]