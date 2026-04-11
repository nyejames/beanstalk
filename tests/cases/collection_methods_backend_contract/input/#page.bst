-- Backend-shape contract case: assert helper/lowering shape, not collection semantics.

values ~= {10, 20, 30}
~values.push(40)
~values.set(0, 99)
~values.remove(1)
first = values.get(0) ! 0
len = values.length()

[:collection_methods_backend_contract first=[first] len=[len]]
