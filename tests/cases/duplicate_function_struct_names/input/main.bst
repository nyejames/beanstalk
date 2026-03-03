-- Test duplicate names between function and struct should be detected

MyType |value Int|

MyType ||:
    ["This is a function with the same name as the struct"]
;

["Testing"]
