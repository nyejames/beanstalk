-- Backend-shape contract case: keep explicit helper checks for collection builtins.

values ~= {10, 20, 30}
~values.push(40)
~values.set(0, 99)
~values.remove(1)
first = values.get(0) ! 0
len = values.length()

[:collection_methods_backend_contract first=[first] len=[len]]
