-- AST rejects multiple-return value passed to io()

multi || -> String, Int:
    return "hello", 42
;

io(multi())
